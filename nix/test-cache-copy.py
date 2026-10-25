#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run the hook that copies cached Cargo target directories from the Nix store into a build, with two read-only
caches that contain the same files.

The later cache must overwrite the shared files of the earlier one, and the copies must be writable. The caches in
the store must not change, and the host and target build locks must be separate files.
"""

import argparse
import fcntl
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

from flake_evaluation import locked_nixpkgs

ROOT = Path(__file__).resolve().parents[1]
NIXPKGS = None

LOCK_PATHS = ["packaged/.cargo-lock", "x86_64-unknown-linux-gnu/packaged/.cargo-lock"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def hook(previous):
    caches = "[ " + " ".join(json.dumps(str(cache)) for cache in previous) + " ]"
    expression = f'''
      let pkgs = import (builtins.toPath {json.dumps(str(NIXPKGS))}) {{ system = "x86_64-linux"; }};
          artifacts = import (builtins.toPath {json.dumps(str(ROOT / "nix/artifacts.nix"))}) {{
            inherit pkgs; sources = {{}}; common = {{}}; skia = null;
          }};
      in artifacts.copy {caches}
    '''
    result = subprocess.run(["nix", "eval", "--offline", "--impure", "--raw", "--expr", expression],
                            text=True, capture_output=True, check=True)
    return result.stdout


class CacheCopyTests(unittest.TestCase):
    def setUp(self):
        self.assertNotEqual(os.geteuid(), 0, "read-only cache regressions require an unprivileged user")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.native = self.directory / "native"
        self.engine = self.directory / "engine"
        for cache, own in [(self.native, "native"), (self.engine, "engine")]:
            files = {
                ".rustc_info.json": own,
                "x86_64-unknown-linux-gnu/packaged/build/shared/output": own,
                f"x86_64-unknown-linux-gnu/packaged/build/{own}/nested/output": own,
            }
            for relative, contents in files.items():
                path = cache / "target" / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents)
            locks = [cache / "target" / relative for relative in LOCK_PATHS]
            for lock in locks:
                lock.parent.mkdir(parents=True, exist_ok=True)
            locks[0].touch()
            os.link(locks[0], locks[1])
            self.assertEqual(locks[0].stat().st_ino, locks[1].stat().st_ino)
            for path in cache.rglob("*"):
                path.chmod(0o555 if path.is_dir() else 0o444)
        self.inputs = {str(path): digest(path) for cache in [self.native, self.engine]
                       for path in cache.rglob("*") if path.is_file()}

    def execute(self, previous):
        script = hook(previous)
        destination = self.directory / "consumer"
        destination.mkdir()
        result = subprocess.run([shutil.which("bash"), "-eu", "-c", script], cwd=destination,
                                text=True, capture_output=True)
        return result, destination / "target"

    def assert_independent_locks(self, target):
        locks = [target / relative for relative in LOCK_PATHS]
        self.assertNotEqual(locks[0].stat().st_ino, locks[1].stat().st_ino)
        with locks[0].open("r+") as host, locks[1].open("r+") as cross:
            fcntl.flock(host, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(cross, fcntl.LOCK_SH | fcntl.LOCK_NB)
            fcntl.flock(cross, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def assert_merge(self, previous, expected):
        result, target = self.execute(previous)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((target / ".rustc_info.json").read_text(), expected)
        self.assertEqual((target / "x86_64-unknown-linux-gnu/packaged/build/shared/output").read_text(), expected)
        for own in ["native", "engine"]:
            self.assertEqual((target / f"x86_64-unknown-linux-gnu/packaged/build/{own}/nested/output").read_text(), own)
        for path in [target] + list(target.rglob("*")):
            self.assertTrue(path.stat().st_mode & stat.S_IWUSR, f"copied target remains read-only: {path}")
        for path, expected_hash in self.inputs.items():
            self.assertEqual(digest(Path(path)), expected_hash)
            self.assertFalse(Path(path).stat().st_mode & stat.S_IWUSR, "source cache was modified")
        self.assert_independent_locks(target)

    def test_engine_overwrites_shared_files_and_adds_directories(self):
        self.assert_merge([self.native, self.engine], "engine")

    def test_reversed_order_keeps_last_cache_precedence(self):
        self.assert_merge([self.engine, self.native], "native")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", type=Path, help="the nixpkgs source (default: the one the flake locks)")
    args, remaining = parser.parse_known_args()
    NIXPKGS = args.nixpkgs or Path(locked_nixpkgs())
    unittest.main(argv=[__file__] + remaining)
