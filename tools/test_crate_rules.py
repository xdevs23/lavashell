# SPDX-License-Identifier: GPL-3.0-only

from pathlib import Path
import tempfile
import unittest

from crate_rules import failures_of

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


def failures(workspace: str, member: str) -> list[str]:
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "Cargo.toml").write_text(workspace)
        crate = root / "crates/member"
        crate.mkdir(parents=True)
        (crate / "Cargo.toml").write_text(member)
        return failures_of(root)


class CrateRules(unittest.TestCase):
    def test_a_conforming_workspace_passes(self) -> None:
        self.assertEqual(failures(WORKSPACE, MEMBER), [])

    def test_a_table_with_an_exact_version_passes(self) -> None:
        exact = WORKSPACE.replace('"=1.0.0"', '{ version = "=1.0.0", features = [] }')
        self.assertEqual(failures(exact, MEMBER), [])

    def test_a_dependency_range_fails(self) -> None:
        message = ["Cargo.toml: workspace dependency exact is not an exact version or a path"]
        self.assertEqual(failures(WORKSPACE.replace("=1.0.0", "1.0.0"), MEMBER), message)

    def test_a_table_with_a_range_fails(self) -> None:
        message = ["Cargo.toml: workspace dependency exact is not an exact version or a path"]
        ranged = WORKSPACE.replace('"=1.0.0"', '{ version = "^1.0", features = [] }')
        self.assertEqual(failures(ranged, MEMBER), message)

    def test_a_member_without_a_path_entry_fails(self) -> None:
        unlisted = WORKSPACE.replace('member = { path = "crates/member" }\n', "")
        message = ["Cargo.toml: member crates/member is not a path entry of the workspace dependencies"]
        self.assertEqual(failures(unlisted, MEMBER), message)

    def test_a_member_dependency_of_its_own_fails(self) -> None:
        own = MEMBER.replace("exact.workspace = true", 'exact = "=1.0.0"')
        message = ["crates/member: dependency exact is not taken from the workspace"]
        self.assertEqual(failures(WORKSPACE, own), message)

    def test_a_target_dependency_of_its_own_fails(self) -> None:
        own = MEMBER + '\n[target.\'cfg(unix)\'.dependencies]\nother = "=2.0.0"\n'
        message = ["crates/member: dependency other is not taken from the workspace"]
        self.assertEqual(failures(WORKSPACE, own), message)

    def test_a_member_without_workspace_lints_fails(self) -> None:
        without_lints = MEMBER.replace("[lints]\nworkspace = true\n", "")
        message = ["crates/member: lints are not taken from the workspace"]
        self.assertEqual(failures(WORKSPACE, without_lints), message)


if __name__ == "__main__":
    unittest.main()
