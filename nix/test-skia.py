#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import json
import os
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

from flake_evaluation import evaluate as nix_eval

ROOT = Path(__file__).resolve().parent.parent
NAME = "skia-fixture"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


class ActivationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.store = self.directory / "store"
        self.cache = self.directory / "cache"
        self.build = self.build_path("0000-" + NAME)
        expression = f"""import {ROOT}/nix/skia-activate.nix {{
          name = {json.dumps(NAME)};
          skia = {json.dumps(str(self.build))};
          storeDir = {json.dumps(str(self.store))};
        }}"""
        evaluated = nix_eval(expression)
        self.assertEqual(evaluated.returncode, 0, evaluated.stderr)
        self.script = self.directory / "activate"
        self.script.write_text(evaluated.stdout)

    def build_path(self, name):
        build = self.store / name
        (build / "lib").mkdir(parents=True)
        (build / "nix-support").mkdir()
        for library in ("skia", "skshaper"):
            (build / "lib" / f"lib{library}.a").write_text(library)
            (build / "lib" / f"lib{library}.a").chmod(0o444)
        (build / "nix-support/skia-environment").write_text("export SKIA_SOURCE_DIR=/fixture/source\n")
        return build

    def activate(self, setup="", then="&&"):
        shell = f'{setup}. {self.script} {then} printf "%s\\n%s\\n" "$SKIA_LIBRARY_SEARCH_PATH" "$SKIA_SOURCE_DIR"'
        environment = dict(os.environ, XDG_CACHE_HOME=str(self.cache))
        return subprocess.run(["bash", "-c", shell], env=environment, capture_output=True, text=True)

    def copies(self):
        return self.cache / "lavashell" / NAME

    def test_a_first_start_makes_a_complete_writable_copy_and_a_second_one_reuses_it(self):
        first = self.activate()
        self.assertEqual(first.returncode, 0, first.stderr)
        copy = Path(first.stdout.splitlines()[0])
        self.assertEqual(copy, self.copies() / self.build.name)
        self.assertEqual(first.stdout.splitlines()[1], "/fixture/source")
        self.assertEqual(sorted(path.name for path in copy.iterdir()), ["libskia.a", "libskshaper.a"])
        self.assertTrue(all(os.access(path, os.W_OK) for path in copy.iterdir()))
        inode = (copy / "libskia.a").stat().st_ino
        second = self.activate()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual((copy / "libskia.a").stat().st_ino, inode)

    def test_a_copy_made_by_another_shell_meanwhile_is_kept_and_the_staged_one_removed(self):
        target = self.copies() / self.build.name
        target.mkdir(parents=True)
        (target / "libskia.a").write_text("other shell")
        result = self.activate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((target / "libskia.a").read_text(), "other shell")
        self.assertEqual([path.name for path in self.copies().iterdir()], [self.build.name])

    def test_a_shell_with_errexit_that_loses_the_race_for_the_first_copy_keeps_the_other_copy(self):
        target = self.copies() / self.build.name
        other_shell = (f'install() {{ command install "$@" && mkdir -p {target} '
                       f'&& echo "other shell" > {target}/libskia.a; }}; set -e; ')
        result = self.activate(other_shell, then=";")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[0], str(target))
        self.assertEqual((target / "libskia.a").read_text(), "other shell\n")
        self.assertEqual([path.name for path in self.copies().iterdir()], [self.build.name])

    def test_an_unwritable_cache_fails_the_activation(self):
        self.cache.mkdir()
        self.cache.chmod(0o500)
        self.addCleanup(self.cache.chmod, 0o700)
        result = self.activate()
        self.assertNotEqual(result.returncode, 0)

    def test_only_copies_of_store_paths_that_are_gone_are_removed(self):
        live = self.build_path("1111-" + NAME)
        for name in (live.name, "2222-" + NAME, ".staged"):
            (self.copies() / name).mkdir(parents=True)
        result = self.activate()
        self.assertEqual(result.returncode, 0, result.stderr)
        remaining = sorted(path.name for path in self.copies().iterdir())
        self.assertEqual(remaining, sorted([".staged", live.name, self.build.name]))


class DriftTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        evaluated = nix_eval(f"(import {ROOT}/nix/skia-bindings.nix).cargoSource")
        if evaluated.returncode != 0:
            raise RuntimeError(evaluated.stderr)
        cls.source = evaluated.stdout

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.workspace = Path(temporary.name)
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.features = manifest["workspace"]["dependencies"]["skia-safe"]["features"]

    def write(self, features, member_features=(), bindings="0.153.3", member="renderer", bindings_source=None,
              safe_source=None):
        quoted = ", ".join(json.dumps(feature) for feature in features)
        (self.workspace / "Cargo.toml").write_text(
            f'[workspace]\nmembers = [{json.dumps(member)}]\n\n'
            f'[workspace.dependencies.skia-safe]\nversion = "=0.153.3"\ndefault-features = false\n'
            f'features = [{quoted}]\n')
        (self.workspace / "renderer").mkdir(exist_ok=True)
        extra = ", ".join(json.dumps(feature) for feature in member_features)
        (self.workspace / "renderer/Cargo.toml").write_text(
            f'[package]\nname = "renderer"\n\n[dependencies]\n'
            f'skia-safe = {{ workspace = true, features = [{extra}] }}\n')
        (self.workspace / "Cargo.lock").write_text(
            f'version = 4\n\n[[package]]\nname = "skia-bindings"\nversion = "{bindings}"\n'
            f'source = "{bindings_source or self.source}"\n\n'
            f'[[package]]\nname = "skia-safe"\nversion = "{bindings}"\nsource = "{safe_source or self.source}"\n')

    def evaluate(self, workspaces=None):
        roots = " ".join(str(root) for root in workspaces or [self.workspace])
        expression = f"""(import {ROOT}/nix/skia.nix {{
            inherit pkgs;
            cargoLock = {self.workspace}/Cargo.lock;
            workspaces = [ {roots} ];
          }}).linux.name"""
        return nix_eval(expression)

    def test_a_workspace_that_asks_for_the_built_release_and_features_evaluates(self):
        self.write(self.features)
        result = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_the_features_of_a_second_workspace_count_for_the_consumer(self):
        self.write(self.features)
        consumer = self.workspace / "consumer"
        consumer.mkdir()
        (consumer / "Cargo.toml").write_text('[package]\nname = "consumer"\n')
        self.assertNotEqual(self.evaluate([consumer]).returncode, 0)
        result = self.evaluate([consumer, self.workspace])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_another_locked_release_fails_evaluation(self):
        self.write(self.features, bindings="0.154.0")
        self.assertNotEqual(self.evaluate().returncode, 0)

    def test_bindings_from_crates_io_fail_evaluation(self):
        self.write(self.features, bindings_source=CRATES_IO)
        self.assertNotEqual(self.evaluate().returncode, 0)

    def test_skia_safe_from_crates_io_fails_evaluation(self):
        self.write(self.features, safe_source=CRATES_IO)
        self.assertNotEqual(self.evaluate().returncode, 0)

    def test_a_feature_added_by_a_member_fails_evaluation(self):
        self.write(self.features, member_features=["svg"])
        self.assertNotEqual(self.evaluate().returncode, 0)

    def test_a_missing_feature_fails_evaluation(self):
        self.write(self.features[1:])
        self.assertNotEqual(self.evaluate().returncode, 0)

    def test_a_member_glob_fails_evaluation(self):
        self.write(self.features, member="crates/*")
        self.assertNotEqual(self.evaluate().returncode, 0)


if __name__ == "__main__":
    unittest.main()
