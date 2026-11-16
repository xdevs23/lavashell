#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import unittest
from dataclasses import dataclass

from document_tables import DocumentTable, TomlTable
from nix_evaluation import nix_eval

GIT: TomlTable = {"git": "https://example.invalid/repo", "rev": "0123abc"}

KEPT_PATCH = """
  { nixpkgs, patch, lock }:
  import ./cargo-patch.nix { lib = import (nixpkgs + "/lib"); } patch lock
"""


@dataclass(frozen=True)
class LockedName:
    name: str
    version: str


@dataclass(frozen=True)
class LockedNames:
    package: tuple[LockedName, ...]


@dataclass(frozen=True)
class PatchArguments:
    patch: TomlTable
    lock: LockedNames


class PatchTests(unittest.TestCase):
    def kept(self, patch: TomlTable, locked: list[str]) -> TomlTable:
        lock = LockedNames(tuple(LockedName(name, "1.0.0") for name in locked))
        return DocumentTable.decoded(nix_eval(KEPT_PATCH, PatchArguments(patch, lock)), "kept patch").toml_table()

    def test_the_patch_of_a_locked_crate_is_kept(self) -> None:
        patch: TomlTable = {"crates-io": {"locked": GIT}}
        self.assertEqual(self.kept(patch, ["locked"]), patch)

    def test_the_patch_of_a_crate_absent_from_the_lock_is_dropped_with_its_empty_table(self) -> None:
        patch: TomlTable = {"crates-io": {"locked": GIT}, "other-registry": {"absent": GIT}}
        self.assertEqual(self.kept(patch, ["locked"]), {"crates-io": {"locked": GIT}})

    def test_a_renamed_patch_is_matched_by_its_package_name(self) -> None:
        renamed: TomlTable = GIT | {"package": "locked"}
        self.assertEqual(self.kept({"crates-io": {"alias": renamed}}, ["locked"]), {"crates-io": {"alias": renamed}})
        self.assertEqual(self.kept({"crates-io": {"locked-alias": renamed}}, ["locked-alias"]), {})


if __name__ == "__main__":
    unittest.main()
