#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import fcntl
import hashlib
import os
import stat
import subprocess
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path

from nix_evaluation import nix_eval_string

LOCK_PATHS = ["packaged/.cargo-lock", "x86_64-unknown-linux-gnu/packaged/.cargo-lock"]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


COPY_SCRIPT = """
  { nixpkgs, caches }:
  (import ./cache-copy.nix { lib = import (nixpkgs + "/lib"); }) caches
"""


@dataclass(frozen=True)
class CopyArguments:
    caches: tuple[Path, ...]


def hook(previous: list[Path]) -> str:
    return nix_eval_string(COPY_SCRIPT, CopyArguments(tuple(previous)))


class CacheCopyTests(unittest.TestCase):
    def setUp(self) -> None:
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
        self.inputs = {path: digest(path) for cache in [self.native, self.engine]
                       for path in cache.rglob("*") if path.is_file()}

    def execute(self, previous: list[Path]) -> tuple[subprocess.CompletedProcess[str], Path]:
        script = hook(previous)
        destination = self.directory / "consumer"
        destination.mkdir()
        result = subprocess.run(["bash", "-eu", "-c", script], cwd=destination,
                                text=True, capture_output=True)
        return result, destination / "target"

    def assert_independent_locks(self, target: Path) -> None:
        locks = [target / relative for relative in LOCK_PATHS]
        self.assertNotEqual(locks[0].stat().st_ino, locks[1].stat().st_ino)
        with locks[0].open("r+") as host, locks[1].open("r+") as cross:
            fcntl.flock(host, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(cross, fcntl.LOCK_SH | fcntl.LOCK_NB)
            fcntl.flock(cross, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def assert_merge(self, previous: list[Path], expected: str) -> None:
        result, target = self.execute(previous)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((target / ".rustc_info.json").read_text(), expected)
        self.assertEqual((target / "x86_64-unknown-linux-gnu/packaged/build/shared/output").read_text(), expected)
        for own in ["native", "engine"]:
            self.assertEqual((target / f"x86_64-unknown-linux-gnu/packaged/build/{own}/nested/output").read_text(), own)
        for path in [target] + list(target.rglob("*")):
            self.assertTrue(path.stat().st_mode & stat.S_IWUSR, f"copied target remains read-only: {path}")
        for source, expected_hash in self.inputs.items():
            self.assertEqual(digest(source), expected_hash)
            self.assertFalse(source.stat().st_mode & stat.S_IWUSR, "source cache was modified")
        self.assert_independent_locks(target)

    def test_engine_overwrites_shared_files_and_adds_directories(self) -> None:
        self.assert_merge([self.native, self.engine], "engine")

    def test_reversed_order_keeps_last_cache_precedence(self) -> None:
        self.assert_merge([self.engine, self.native], "native")


if __name__ == "__main__":
    unittest.main()
