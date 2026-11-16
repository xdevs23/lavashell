#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import unittest

from cargo_models import PackageId


class PackageIdTests(unittest.TestCase):
    def test_a_registry_package_names_itself_in_the_fragment(self) -> None:
        package = PackageId.parse("registry+https://github.com/rust-lang/crates.io-index#bevy_app@0.17.3")
        self.assertEqual(package, PackageId("bevy_app", "0.17.3"))

    def test_a_git_package_with_a_query_names_itself_in_the_fragment(self) -> None:
        package = PackageId.parse("git+https://example.org/skia-bindings?rev=abc123#skia-bindings@0.90.0")
        self.assertEqual(package, PackageId("skia-bindings", "0.90.0"))

    def test_a_path_package_without_a_name_in_the_fragment_is_named_by_its_directory(self) -> None:
        package = PackageId.parse("path+file:///build/source/crates/lavashell-core#0.1.0")
        self.assertEqual(package, PackageId("lavashell-core", "0.1.0"))


if __name__ == "__main__":
    unittest.main()
