#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

import check_cargo_reuse as reuse
from cargo_models import CompilerArtifact
from document_tables import DocumentTable
from verification_errors import (ArtifactIdentitiesChanged, BuildNotFinished, DependencyRebuilt,
                                 VerificationError)

PROFILE = {"opt_level": "2", "debuginfo": 0, "debug_assertions": False, "overflow_checks": False, "test": False}


def artifact(target: str, kind: str, fresh: bool) -> CompilerArtifact:
    return CompilerArtifact.from_json(DocumentTable.decoded({
        "package_id": "registry+https://github.com/rust-lang/crates.io-index#bevy_app@0.17.3",
        "target": {"name": target, "kind": [kind]}, "profile": PROFILE,
        "features": ["std"], "filenames": [f"/build/source/target/{target}"], "fresh": fresh}, "artifact"))


class CargoReuse(unittest.TestCase):
    def setUp(self) -> None:
        self.reference = [artifact("bevy_app", "lib", False), artifact("build-script-build", "custom-build", False)]
        self.current = [artifact("bevy_app", "lib", True), artifact("build-script-build", "custom-build", True)]

    def test_both_library_and_build_script_must_be_fresh(self) -> None:
        self.assertEqual(reuse.verify(self.reference, self.current, "bevy", None), 2)
        self.current[1] = replace(self.current[1], fresh=False)
        with self.assertRaises(VerificationError) as failure:
            reuse.verify(self.reference, self.current, "bevy", None)
        self.assertEqual(failure.exception.cause, DependencyRebuilt())

    def test_missing_or_changed_feature_units_are_rejected(self) -> None:
        with self.assertRaises(VerificationError) as missing:
            reuse.verify(self.reference, self.current[:1], "bevy", None)
        self.assertEqual(missing.exception.cause, ArtifactIdentitiesChanged())
        changed = [replace(self.current[0], features=(*self.current[0].features, "reflect")), self.current[1]]
        with self.assertRaises(VerificationError) as failure:
            reuse.verify(self.reference, changed, "bevy", None)
        self.assertEqual(failure.exception.cause, ArtifactIdentitiesChanged())

    def test_incomplete_build_output_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "artifacts.jsonl"
            path.write_text('{"reason":"build-finished","success":false}\n')
            with self.assertRaises(VerificationError) as failure:
                reuse.records(path)
        self.assertEqual(failure.exception.cause, BuildNotFinished(path=path))


if __name__ == "__main__":
    unittest.main()
