#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import json
import unittest
from pathlib import Path

from flake_evaluation import evaluate

ROOT = Path(__file__).resolve().parent.parent
GIT = {"git": "https://example.invalid/repo", "rev": "0123abc"}


class PatchTests(unittest.TestCase):
    def kept(self, patch, locked):
        lock = {"package": [{"name": name, "version": "1.0.0"} for name in locked]}
        expression = f"""
          import {ROOT}/nix/cargo-patch.nix {{ inherit (pkgs) lib; }}
            (builtins.fromJSON {json.dumps(json.dumps(patch))})
            (builtins.fromJSON {json.dumps(json.dumps(lock))})"""
        result = evaluate(expression, "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_the_patch_of_a_locked_crate_is_kept(self):
        patch = {"crates-io": {"locked": GIT}}
        self.assertEqual(self.kept(patch, ["locked"]), patch)

    def test_the_patch_of_a_crate_absent_from_the_lock_is_dropped_with_its_empty_table(self):
        patch = {"crates-io": {"locked": GIT}, "other-registry": {"absent": GIT}}
        self.assertEqual(self.kept(patch, ["locked"]), {"crates-io": {"locked": GIT}})

    def test_a_renamed_patch_is_matched_by_its_package_name(self):
        renamed = GIT | {"package": "locked"}
        self.assertEqual(self.kept({"crates-io": {"alias": renamed}}, ["locked"]), {"crates-io": {"alias": renamed}})
        self.assertEqual(self.kept({"crates-io": {"locked-alias": renamed}}, ["locked-alias"]), {})


if __name__ == "__main__":
    unittest.main()
