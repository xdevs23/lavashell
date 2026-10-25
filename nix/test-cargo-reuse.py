#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("reuse", Path(__file__).with_name("check-cargo-reuse.py"))
reuse = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reuse)


def artifact(target, kind, fresh):
    return {"package_id": "registry+https://github.com/rust-lang/crates.io-index#bevy_app@0.17.3",
            "target": {"name": target, "kind": [kind]}, "profile": {"opt_level": "2"},
            "features": ["std"], "filenames": [f"/build/source/target/{target}"], "fresh": fresh}


class CargoReuse(unittest.TestCase):
    def setUp(self):
        self.reference = [artifact("bevy_app", "lib", False), artifact("build-script-build", "custom-build", False)]
        self.current = [artifact("bevy_app", "lib", True), artifact("build-script-build", "custom-build", True)]

    def test_both_library_and_build_script_must_be_fresh(self):
        self.assertEqual(reuse.verify(self.reference, self.current, "bevy"), 2)
        self.current[1]["fresh"] = False
        with self.assertRaises(AssertionError):
            reuse.verify(self.reference, self.current, "bevy")

    def test_missing_or_changed_feature_units_are_rejected(self):
        with self.assertRaises(AssertionError):
            reuse.verify(self.reference, self.current[:1], "bevy")
        changed = copy.deepcopy(self.current)
        changed[0]["features"].append("reflect")
        with self.assertRaises(AssertionError):
            reuse.verify(self.reference, changed, "bevy")

    def test_incomplete_build_output_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "artifacts.jsonl"
            path.write_text('{"reason":"build-finished","success":false}\n')
            with self.assertRaises(AssertionError):
                reuse.records(path)


if __name__ == "__main__":
    unittest.main()
