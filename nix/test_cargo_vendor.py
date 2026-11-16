#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path

import tomli_w

from cargo_files import CargoLock, LockedPackage
from nix_evaluation import NixEvaluationError, nix_eval_string

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
GIT = "git+https://example.invalid/repo?rev=0123abc#0123abc"
HASH = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="

VENDORED_LOCK = """
  { nixpkgs, lock, commits }:
  (import ./cargo-vendor.nix { pkgs = import nixpkgs { }; inherit commits; } (/. + lock)).drvPath
"""


@dataclass(frozen=True)
class VendoredCommit:
    cargo_source: str
    hash: str


@dataclass(frozen=True)
class VendorArguments:
    lock: Path
    commits: tuple[VendoredCommit, ...]


def locked(name: str, source: str) -> LockedPackage:
    return LockedPackage(name=name, version="1.0.0", source=source,
                         checksum="0" * 64 if source == CRATES_IO else None, dependencies=())


class VendorTests(unittest.TestCase):
    def evaluate(self, packages: list[tuple[str, str]], commits: tuple[VendoredCommit, ...]) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "Cargo.lock"
            packaged = CargoLock(version=4, packages=tuple(locked(name, source) for name, source in packages))
            lock.write_text(tomli_w.dumps(packaged.to_toml()))
            self.assertTrue(nix_eval_string(VENDORED_LOCK, VendorArguments(lock, commits)).endswith(".drv"))

    def test_crates_from_crates_io_and_a_known_commit_are_vendored(self) -> None:
        self.evaluate([("registry-crate", CRATES_IO), ("git-crate", GIT)], (VendoredCommit(GIT, HASH),))

    def test_a_lock_without_git_crates_needs_no_commits(self) -> None:
        self.evaluate([("registry-crate", CRATES_IO)], ())

    def test_a_commit_without_a_hash_fails_evaluation_naming_the_crate(self) -> None:
        with self.assertRaises(NixEvaluationError) as failure:
            self.evaluate([("git-crate", GIT)], ())
        self.assertIn("git-crate", failure.exception.stderr)


if __name__ == "__main__":
    unittest.main()
