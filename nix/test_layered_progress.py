#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import check_cargo_reuse as reuse
import verify_layered_build as layered
from verification_errors import DependencyRebuilt, SandboxBuildFailed, VerificationError

PROFILE = {"opt_level": "2", "debuginfo": 0, "debug_assertions": False, "overflow_checks": False, "test": False}

FAKE_TOOL = """import json
import os
import sys
import tarfile
from pathlib import Path

tool, command = Path(sys.argv[0]).name, sys.argv[1]
if tool == "git" and command == "rev-parse":
    print("frozen-commit")
elif tool == "git" and command == "archive":
    tarfile.open(fileobj=sys.stdout.buffer, mode="w|").close()
elif tool == "nix" and command == "store":
    print("/fixture/source")
elif tool == "nix-instantiate":
    print(json.dumps({"reference": "/fixture/reference.drv", "engine": "/fixture/engine.drv",
                      "registry": "/fixture/registry.drv", "framework": "/fixture/framework.drv",
                      "engineOut": "/fixture/engine", "frameworkOut": "/fixture/framework",
                      "referenceOut": "/fixture/reference", "directory": "/fixture/source/examples/reference"}))
elif tool == "nix" and command == "build":
    if "-L" in sys.argv:
        print("compiler output", file=sys.stderr)
    sys.exit(int(os.environ["FAKE_BUILD_STATUS"]))
"""


class LayeredProgressTests(unittest.TestCase):
    def baseline_artifact(self, directory: str, name: str, fresh: bool) -> Path:
        path = Path(directory) / f"{name}.jsonl"
        record = {"reason": "compiler-artifact",
                  "package_id": "registry+https://github.com/rust-lang/crates.io-index#bevy_app@0.17.3",
                  "target": {"name": "bevy_app", "kind": ["lib"]}, "profile": PROFILE,
                  "features": ["std"], "filenames": ["/build/source/target/libbevy_app.rlib"], "fresh": fresh}
        path.write_text(json.dumps(record) + '\n{"reason":"build-finished","success":true}\n')
        return path

    def with_fake_tools(self, directory: str, build_status: int) -> None:
        tools = Path(directory) / "tools"
        tools.mkdir()
        for name in ("git", "nix", "nix-instantiate"):
            (tools / name).write_text(f"#!{sys.executable}\n{FAKE_TOOL}")
            (tools / name).chmod(stat.S_IRWXU)
        environment = {"PATH": f"{tools}:{os.environ['PATH']}", "LAVASHELL_NIXPKGS": "/fixture/nixpkgs",
                       "FAKE_BUILD_STATUS": str(build_status)}
        patcher = mock.patch.dict(os.environ, environment)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_final_fresh_cannot_hide_framework_recompilation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            engine = self.baseline_artifact(directory, "engine", False)
            framework = self.baseline_artifact(directory, "framework", False)
            reference = self.baseline_artifact(directory, "reference", True)
            self.assertEqual(reuse.verify(reuse.records(engine), reuse.records(reference), "bevy", None), 1)
            with self.assertRaises(VerificationError) as failure:
                layered.verify_baseline_reuse(engine, framework, reference)
            self.assertEqual(failure.exception.cause, DependencyRebuilt())

    def test_framework_and_final_freshness_are_both_recorded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            engine = self.baseline_artifact(directory, "engine", False)
            framework = self.baseline_artifact(directory, "framework", True)
            reference = self.baseline_artifact(directory, "reference", True)
            self.assertEqual(layered.verify_baseline_reuse(engine, framework, reference),
                             layered.BaselineReuse(fresh_engine_in_framework=1, fresh_engine_in_baseline=1))

    def test_the_log_of_the_build_contains_the_compiler_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.with_fake_tools(directory, build_status=0)
            log = Path(directory) / "baseline.log"
            wall_seconds = layered.build("/fixture/source", log)
            self.assertEqual(log.read_text(), "compiler output\n")
            self.assertGreaterEqual(wall_seconds, 0)

    def test_baseline_failure_leaves_incomplete_progress_before_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.with_fake_tools(directory, build_status=3)
            output = Path(directory) / "output"
            with self.assertRaises(VerificationError) as failure:
                layered.main(["--output", str(output)])
            self.assertEqual(failure.exception.cause, SandboxBuildFailed(code=3, log=output / "baseline.log"))
            report = json.loads((output / "report.json").read_text())
        self.assertFalse(report["complete"])
        self.assertEqual(report["phase"], "baseline_build")
        self.assertEqual(report["baseline"]["source"], "/fixture/source")


if __name__ == "__main__":
    unittest.main()
