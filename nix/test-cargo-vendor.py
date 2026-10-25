#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import json
import tempfile
import unittest
from pathlib import Path

from flake_evaluation import evaluate

ROOT = Path(__file__).resolve().parent.parent
GIT = "git+https://example.invalid/repo?rev=0123abc#0123abc"
HASH = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="


class VendorTests(unittest.TestCase):
    def evaluate(self, sources, commits):
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "Cargo.lock"
            lock.write_text("version = 4\n\n" + "".join(
                f'[[package]]\nname = "{name}"\nversion = "1.0.0"\nsource = "{source}"\n'
                + (f'checksum = "{"0" * 64}"\n' if source.startswith("registry+") else "") + "\n"
                for name, source in sources.items()))
            return evaluate(f"""
              (import {ROOT}/nix/cargo-vendor.nix {{
                inherit pkgs;
                commits = builtins.fromJSON {json.dumps(json.dumps(commits))};
              }} {lock}).drvPath""")

    def test_crates_from_crates_io_and_a_known_commit_are_vendored(self):
        sources = {"registry-crate": "registry+https://github.com/rust-lang/crates.io-index", "git-crate": GIT}
        result = self.evaluate(sources, [{"cargoSource": GIT, "hash": HASH}])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_lock_without_git_crates_needs_no_commits(self):
        result = self.evaluate({"registry-crate": "registry+https://github.com/rust-lang/crates.io-index"}, [])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_commit_without_a_hash_fails_evaluation(self):
        result = self.evaluate({"git-crate": GIT}, [])
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
