#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

spec = importlib.util.spec_from_file_location("layered", Path(__file__).with_name("verify-layered-build.py"))
layered = importlib.util.module_from_spec(spec)
spec.loader.exec_module(layered)
reuse_spec = importlib.util.spec_from_file_location("reuse", Path(__file__).with_name("check-cargo-reuse.py"))
reuse = importlib.util.module_from_spec(reuse_spec)
reuse_spec.loader.exec_module(reuse)


class LayeredProgressTests(unittest.TestCase):
    def baseline_artifact(self, directory, name, fresh):
        path = Path(directory) / f"{name}.jsonl"
        record = {"reason": "compiler-artifact",
                  "package_id": "registry+https://github.com/rust-lang/crates.io-index#bevy_app@0.17.3",
                  "target": {"name": "bevy_app", "kind": ["lib"]}, "profile": {"opt_level": "2"},
                  "features": ["std"], "filenames": ["/build/source/target/libbevy_app.rlib"], "fresh": fresh}
        path.write_text(json.dumps(record) + '\n{"reason":"build-finished","success":true}\n')
        return path

    def test_final_fresh_cannot_hide_framework_recompilation(self):
        with tempfile.TemporaryDirectory() as directory:
            engine = self.baseline_artifact(directory, "engine", False)
            framework = self.baseline_artifact(directory, "framework", False)
            reference = self.baseline_artifact(directory, "reference", True)
            self.assertEqual(reuse.verify(reuse.records(engine), reuse.records(reference), "bevy"), 1)
            with self.assertRaisesRegex(AssertionError, "Cargo rebuilt a dependency"):
                layered.verify_baseline_reuse(engine, framework, reference, reuse)

    def test_framework_and_final_freshness_are_both_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            engine = self.baseline_artifact(directory, "engine", False)
            framework = self.baseline_artifact(directory, "framework", True)
            reference = self.baseline_artifact(directory, "reference", True)
            self.assertEqual(layered.verify_baseline_reuse(engine, framework, reference, reuse),
                             {"fresh_bevy_in_framework": 1, "fresh_bevy_in_baseline": 1})

    def test_build_enables_visible_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                mock.patch.object(layered.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as call,
                mock.patch.object(layered.time, "monotonic", side_effect=[20.0, 21.25]),
            ):
                wall_seconds = layered.build("/fixture/source", "/fixture/nixpkgs", Path(directory) / "baseline.log")
            command = layered.build_command("/fixture/source", "/fixture/nixpkgs")
            self.assertIn("-L", command)
            self.assertEqual(wall_seconds, 1.25)
            self.assertEqual(call.call_args.args[0], command)
            self.assertEqual(call.call_args.kwargs["stderr"], subprocess.STDOUT)

    def test_baseline_failure_leaves_incomplete_progress_before_build(self):
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w"):
            pass
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            observed = []

            def stop_build(source, nixpkgs, log):
                report = json.loads((output / "report.json").read_text())
                observed.append(report)
                self.assertFalse(report["complete"])
                self.assertEqual(report["phase"], "baseline_build")
                self.assertEqual(report["baseline"]["source"], source)
                self.assertIn("-L", report["baseline"]["command"])
                raise RuntimeError("intentional baseline failure")

            arguments = [__file__, "--nixpkgs", "/fixture/nixpkgs", "--output", str(output)]
            with mock.patch("sys.argv", arguments), \
                    mock.patch.object(layered, "run", side_effect=lambda command, **_: (
                        "frozen-commit" if command[1] == "rev-parse" else "")), \
                    mock.patch.object(layered.subprocess, "run", return_value=subprocess.CompletedProcess(
                        [], 0, stdout=archive.getvalue())), \
                    mock.patch.object(layered, "snapshot", return_value="/fixture/source"), \
                    mock.patch.object(layered, "evaluate", return_value={"reference": "/fixture/reference.drv"}), \
                    mock.patch.object(layered, "build", side_effect=stop_build):
                with self.assertRaisesRegex(RuntimeError, "intentional baseline failure"):
                    layered.main()
            self.assertEqual(len(observed), 1)
            self.assertFalse(json.loads((output / "report.json").read_text())["complete"])


if __name__ == "__main__":
    unittest.main()
