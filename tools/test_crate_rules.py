# SPDX-License-Identifier: GPL-3.0-only
"""Tests of `crate_rules` on small workspaces written into temporary directories."""

from pathlib import Path
import tempfile
import unittest

from crate_rules import failures_of, is_pinned

WORKSPACE = """
[workspace]
members = ["crates/member"]

[workspace.dependencies]
exact = "=1.0.0"
member = { path = "crates/member" }
"""

MEMBER = """
[package]
name = "member"
version.workspace = true
edition.workspace = true

[lints]
workspace = true

[dependencies]
exact.workspace = true
"""


def failures(workspace, member):
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "Cargo.toml").write_text(workspace)
        crate = root / "crates/member"
        crate.mkdir(parents=True)
        (crate / "Cargo.toml").write_text(member)
        return failures_of(root)


class CrateRules(unittest.TestCase):
    def test_a_conforming_workspace_passes(self):
        self.assertEqual(failures(WORKSPACE, MEMBER), [])

    def test_a_dependency_range_fails(self):
        self.assertFalse(is_pinned("1.0"))
        self.assertFalse(is_pinned({"version": "^1.0", "features": []}))
        self.assertTrue(is_pinned({"version": "=1.0.0", "features": []}))
        self.assertEqual(len(failures(WORKSPACE.replace("=1.0.0", "1.0.0"), MEMBER)), 1)

    def test_a_member_without_a_path_entry_fails(self):
        unlisted = WORKSPACE.replace('member = { path = "crates/member" }\n', "")
        self.assertEqual(len(failures(unlisted, MEMBER)), 1)

    def test_a_member_dependency_of_its_own_fails(self):
        own = MEMBER.replace("exact.workspace = true", 'exact = "=1.0.0"')
        self.assertEqual(len(failures(WORKSPACE, own)), 1)

    def test_a_target_dependency_of_its_own_fails(self):
        own = MEMBER + '\n[target.\'cfg(unix)\'.dependencies]\nother = "=2.0.0"\n'
        self.assertEqual(len(failures(WORKSPACE, own)), 1)

    def test_a_member_without_workspace_lints_fails(self):
        without_lints = MEMBER.replace("[lints]\nworkspace = true\n", "")
        self.assertEqual(len(failures(WORKSPACE, without_lints)), 1)


if __name__ == "__main__":
    unittest.main()
