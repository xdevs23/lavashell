# SPDX-License-Identifier: GPL-3.0-only
"""Tests of the comparison between the fuzz workspace and the root workspace."""

import unittest

from fuzz_workspace import Workspace, differences, versions

ROOT = Workspace(
    "2024", {"serde": "=1.0.229", "lavashell-core": None}, {"rust": {"unused_results": "deny"}}
)


def fuzz(edition: str = "2024", serde: str = "=1.0.229", lints: str = "deny") -> Workspace:
    dependencies = {"serde": serde, "lavashell-core": None, "libfuzzer-sys": "=0.4.13"}
    return Workspace(edition, dependencies, {"rust": {"unused_results": lints}})


class VersionTests(unittest.TestCase):
    def test_a_version_is_read_from_text_or_a_table_and_a_path_has_none(self):
        serde = {"version": "=1.0.229", "features": ["derive"]}
        dependencies = {"log": "=0.4.29", "serde": serde, "core": {"path": ".."}}
        expected = {"log": "=0.4.29", "serde": "=1.0.229", "core": None}
        self.assertEqual(versions(dependencies), expected)


class DifferenceTests(unittest.TestCase):
    def test_copies_that_match_the_root_differ_in_nothing(self):
        self.assertEqual(differences(ROOT, fuzz()), [])

    def test_a_different_edition_version_or_lint_is_reported(self):
        self.assertEqual(len(differences(ROOT, fuzz(edition="2021"))), 1)
        self.assertEqual(len(differences(ROOT, fuzz(serde="=1.0.228"))), 1)
        self.assertEqual(len(differences(ROOT, fuzz(lints="warn"))), 1)


if __name__ == "__main__":
    unittest.main()
