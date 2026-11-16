#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from cargo_models import BuildFinished, OtherOutput, read_message
from nix_evaluation import nixpkgs_source

ROOT = Path(__file__).resolve().parent

SUPPORT = r'''
cargoBuildType=packaged
NIX_BUILD_CORES=1
cargoBuildFlags=(--message-format=json-render-diagnostics)
postBuildHooks=()
export NIX_BUILD_TOP="$PWD" out="$PWD/out"
runHook() {
    case "$1" in
        preBuild) export PKG_CONFIG=retained-pkg-config ;;
        postBuild) for hook in "${postBuildHooks[@]}"; do "$hook"; done ;;
    esac
}
concatTo() {
    local -n destination="$1" source="$2"
    destination+=("${source[@]}")
}
echoCmd() { printf '%s\n' "$*"; }
cargo() {
    if [ "$CARGO_FIXTURE_STATUS" != 0 ]; then
        printf '{"reason":"build-finished","success":false}\n'
        return "$CARGO_FIXTURE_STATUS"
    fi
    mkdir -p target/x86_64-unknown-linux-gnu/packaged
    printf '#!/bin/sh\nexit 0\n' > target/x86_64-unknown-linux-gnu/packaged/fixture
    chmod +x target/x86_64-unknown-linux-gnu/packaged/fixture
    printf '{"reason":"build-finished","success":true}\n'
}
source build-hook.sh
source install-hook.sh
source "$PHASE"
printf 'CHECK_PKG_CONFIG=%s\n' "${PKG_CONFIG-missing}"
printf 'CHECK_BINS=%s\n' "${bins-missing}"
cargoInstallHook
printf 'INSTALL_FINISHED\n'
'''


class CargoHookTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        for source, destination in (("cargo-build-hook.sh", "build-hook.sh"),
                                    ("cargo-install-hook.sh", "install-hook.sh")):
            contents = (nixpkgs_source() / "pkgs/build-support/rust/hooks" / source).read_text()
            contents = contents.replace("@rustcTargetSpec@", "x86_64-unknown-linux-gnu")
            contents = contents.replace("@targetSubdirectory@", "x86_64-unknown-linux-gnu").replace("@setEnv@", "")
            (self.directory / destination).write_text(contents)
        (self.directory / "harness.sh").write_text(SUPPORT)

    def execute(self, *, status: int = 0, full: bool = False) -> subprocess.CompletedProcess[str]:
        if full:
            (self.directory / "cargo-artifacts.jsonl").symlink_to("/dev/full")
        command = ["bash", "-eu", "-c", 'source harness.sh']
        inherited = {key: value for key, value in os.environ.items() if key not in ("PKG_CONFIG", "bins")}
        environment = dict(inherited, PHASE=str(ROOT / "cargo-build-record.sh"), CARGO_FIXTURE_STATUS=str(status))
        return subprocess.run(command, cwd=self.directory, env=environment,
                              text=True, capture_output=True)

    def test_exports_install_bins_and_complete_log_survive(self) -> None:
        result = self.execute()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CHECK_PKG_CONFIG=retained-pkg-config", result.stdout)
        self.assertNotIn("CHECK_BINS=missing", result.stdout)
        self.assertTrue((self.directory / "out/bin/fixture").is_file())
        lines = (self.directory / "cargo-artifacts.jsonl").read_text().splitlines()
        messages = [message for message in map(read_message, lines) if not isinstance(message, OtherOutput)]
        self.assertEqual(messages, [BuildFinished(success=True)])
        self.assertIn("Finished cargoBuildHook", (self.directory / "cargo-artifacts.jsonl").read_text())

    def test_cargo_failure_is_propagated_before_install(self) -> None:
        result = self.execute(status=42)
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertNotIn("INSTALL_FINISHED", result.stdout)
        self.assertNotIn("cargoInstallPostBuildHook", result.stdout)

    def test_tee_write_failure_is_propagated_before_install(self) -> None:
        result = self.execute(full=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("No space left on device", result.stderr)
        self.assertNotIn("INSTALL_FINISHED", result.stdout)


if __name__ == "__main__":
    unittest.main()
