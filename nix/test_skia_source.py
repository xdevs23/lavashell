# SPDX-License-Identifier: GPL-3.0-only

import unittest

from refresh_skia_source import Pin, pinned_externals
from verification_errors import SkiaDepsUnreadable, SkiaExternalUnreadable, VerificationError

REVISION = "0123456789abcdef0123456789abcdef01234567"
URL = "https://example.invalid/fixture.git"


def deps(table: str) -> str:
    return f'vars = {{"fixture_git": "https://example.invalid"}}\ndeps = {{\n{table}\n}}\n'


class PinnedExternals(unittest.TestCase):
    def test_the_url_and_revision_of_a_named_external_are_read(self) -> None:
        source = deps(f'  "third_party/externals/fixture": "{URL}@{REVISION}",')
        self.assertEqual(pinned_externals(source, ["fixture"]), {"fixture": Pin(URL, REVISION)})

    def test_entries_that_are_not_asked_for_may_be_computed(self) -> None:
        source = deps(f'  "third_party/externals/other": Var("fixture_git") + "/other.git@{REVISION}",\n'
                      f'  "third_party/externals/fixture": "{URL}@{REVISION}",')
        self.assertEqual(pinned_externals(source, ["fixture"]), {"fixture": Pin(URL, REVISION)})

    def test_an_external_missing_from_the_table_fails_with_its_name(self) -> None:
        with self.assertRaises(VerificationError) as failure:
            pinned_externals(deps(""), ["fixture"])
        self.assertEqual(failure.exception.cause, SkiaExternalUnreadable(name="fixture"))
        self.assertIn("fixture", str(failure.exception))

    def test_a_computed_entry_of_a_named_external_fails(self) -> None:
        source = deps(f'  "third_party/externals/fixture": Var("fixture_git") + "/fixture.git@{REVISION}",')
        with self.assertRaises(VerificationError) as failure:
            pinned_externals(source, ["fixture"])
        self.assertEqual(failure.exception.cause, SkiaExternalUnreadable(name="fixture"))

    def test_a_file_without_a_deps_table_fails(self) -> None:
        with self.assertRaises(VerificationError) as failure:
            pinned_externals("vars = {}\n", ["fixture"])
        self.assertEqual(failure.exception.cause, SkiaDepsUnreadable())


if __name__ == "__main__":
    unittest.main()
