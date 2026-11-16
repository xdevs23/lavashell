#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import os
import subprocess
import tempfile
import unittest
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

import tomli_w

from cargo_files import CargoLock, LockedPackage
from document_tables import TomlTable
from nix_evaluation import NixEvaluationError, nix_eval, nix_eval_string

NAME = "skia-fixture"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
FORK = "https://example.invalid/skia-fork"
FEATURES = ("gl", "textlayout", "vulkan")


@dataclass(frozen=True)
class Bindings:
    version: str
    crates: tuple[str, ...]
    url: str
    rev: str
    cargo_source: str


BINDINGS = Bindings(version="0.153.3", crates=("skia-bindings", "skia-safe"), url=FORK, rev="0123abc",
                    cargo_source=f"git+{FORK}?rev=0123abc#0123abc")


@dataclass(frozen=True)
class ActivationArguments:
    name: str
    skia: Path
    store_dir: Path


@dataclass(frozen=True)
class LockCheckArguments:
    bindings: Bindings
    features: tuple[str, ...]
    lock: Path
    workspaces: tuple[Path, ...]

ACTIVATION_SCRIPT = """
  { name, skia, storeDir, ... }: import ./skia-activate.nix { inherit name skia storeDir; }
"""

LOCK_CHECK = """
  { nixpkgs, bindings, features, lock, workspaces }:
  import ./skia-lock-check.nix {
    lib = import (nixpkgs + "/lib");
    inherit bindings;
    generated.fork_rev = bindings.rev;
    supportedFeatures = features;
    cargoLock = /. + lock;
    workspaces = map (workspace: /. + workspace) workspaces;
  }
"""


class ActivationTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.store = self.directory / "store"
        self.cache = self.directory / "cache"
        self.build = self.build_path("0000-" + NAME)
        self.script = self.directory / "activate"
        self.script.write_text(nix_eval_string(ACTIVATION_SCRIPT, ActivationArguments(NAME, self.build, self.store)))

    def build_path(self, name: str) -> Path:
        build = self.store / name
        (build / "lib").mkdir(parents=True)
        (build / "nix-support").mkdir()
        for library in ("skia", "skshaper"):
            (build / "lib" / f"lib{library}.a").write_text(library)
            (build / "lib" / f"lib{library}.a").chmod(0o444)
        (build / "nix-support/skia-environment").write_text("export SKIA_SOURCE_DIR=/fixture/source\n")
        return build

    def activate(self, setup: str = "", then: str = "&&") -> subprocess.CompletedProcess[str]:
        shell = f'{setup}. {self.script} {then} printf "%s\\n%s\\n" "$SKIA_LIBRARY_SEARCH_PATH" "$SKIA_SOURCE_DIR"'
        environment = dict(os.environ, XDG_CACHE_HOME=str(self.cache))
        return subprocess.run(["bash", "-c", shell], env=environment, capture_output=True, text=True)

    def copies(self) -> Path:
        return self.cache / "lavashell" / NAME

    def test_a_first_start_makes_a_complete_writable_copy_and_a_second_one_reuses_it(self) -> None:
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

    def test_a_copy_made_by_another_shell_meanwhile_is_kept_and_the_staged_one_removed(self) -> None:
        target = self.copies() / self.build.name
        target.mkdir(parents=True)
        (target / "libskia.a").write_text("other shell")
        result = self.activate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((target / "libskia.a").read_text(), "other shell")
        self.assertEqual([path.name for path in self.copies().iterdir()], [self.build.name])

    def test_a_shell_with_errexit_that_loses_the_race_for_the_first_copy_keeps_the_other_copy(self) -> None:
        target = self.copies() / self.build.name
        other_shell = (f'install() {{ command install "$@" && mkdir -p {target} '
                       f'&& echo "other shell" > {target}/libskia.a; }}; set -e; ')
        result = self.activate(other_shell, then=";")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[0], str(target))
        self.assertEqual((target / "libskia.a").read_text(), "other shell\n")
        self.assertEqual([path.name for path in self.copies().iterdir()], [self.build.name])

    def test_a_cache_that_cannot_hold_the_copies_fails_the_activation_naming_the_cache(self) -> None:
        self.cache.write_text("not a directory")
        result = self.activate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(str(self.cache), result.stderr)

    def test_only_copies_of_store_paths_that_are_gone_are_removed(self) -> None:
        live = self.build_path("1111-" + NAME)
        for name in (live.name, "2222-" + NAME, ".staged"):
            (self.copies() / name).mkdir(parents=True)
        result = self.activate()
        self.assertEqual(result.returncode, 0, result.stderr)
        remaining = sorted(path.name for path in self.copies().iterdir())
        self.assertEqual(remaining, sorted([".staged", live.name, self.build.name]))


class DriftTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.workspace = Path(temporary.name)

    def write(self, features: Sequence[str], member_features: Sequence[str] = (), bindings: str = BINDINGS.version,
              member: str = "renderer", bindings_source: str = BINDINGS.cargo_source,
              safe_source: str = BINDINGS.cargo_source) -> None:
        workspace: TomlTable = {"workspace": {
            "members": [member],
            "dependencies": {"skia-safe": {"version": "=" + BINDINGS.version, "default-features": False,
                                           "features": list(features)}}}}
        (self.workspace / "Cargo.toml").write_text(tomli_w.dumps(workspace))
        (self.workspace / "renderer").mkdir(exist_ok=True)
        (self.workspace / "renderer/Cargo.toml").write_text(tomli_w.dumps({
            "package": {"name": "renderer"},
            "dependencies": {"skia-safe": {"workspace": True, "features": list(member_features)}}}))
        lock = CargoLock(version=4, packages=(
            LockedPackage("skia-bindings", bindings, bindings_source, None, ()),
            LockedPackage("skia-safe", bindings, safe_source, None, ())))
        (self.workspace / "Cargo.lock").write_text(tomli_w.dumps(lock.to_toml()))

    def check(self, workspaces: Sequence[Path]) -> object:
        return nix_eval(LOCK_CHECK, LockCheckArguments(BINDINGS, FEATURES, self.workspace / "Cargo.lock",
                                                       tuple(workspaces)))

    def refuses(self, *causes: str, workspaces: Sequence[Path]) -> None:
        with self.assertRaises(NixEvaluationError) as failure:
            self.check(workspaces)
        for cause in causes:
            self.assertIn(cause, failure.exception.stderr)

    def test_a_workspace_that_asks_for_the_built_release_and_features_is_accepted(self) -> None:
        self.write(FEATURES)
        self.assertIs(self.check([self.workspace]), True)

    def test_the_features_of_a_second_workspace_count_for_the_consumer(self) -> None:
        self.write(FEATURES)
        consumer = self.workspace / "consumer"
        consumer.mkdir()
        (consumer / "Cargo.toml").write_text(tomli_w.dumps({"package": {"name": "consumer"}}))
        self.refuses(*FEATURES, workspaces=[consumer])
        self.assertIs(self.check([consumer, self.workspace]), True)

    def test_another_locked_release_is_refused_naming_the_release(self) -> None:
        self.write(FEATURES, bindings="0.154.0")
        self.refuses("0.154.0", BINDINGS.version, workspaces=[self.workspace])

    def test_bindings_from_crates_io_are_refused_naming_the_source(self) -> None:
        self.write(FEATURES, bindings_source=CRATES_IO)
        self.refuses(CRATES_IO, "skia-bindings", workspaces=[self.workspace])

    def test_skia_safe_from_crates_io_is_refused_naming_the_source(self) -> None:
        self.write(FEATURES, safe_source=CRATES_IO)
        self.refuses(CRATES_IO, "skia-safe", workspaces=[self.workspace])

    def test_a_feature_added_by_a_member_is_refused_naming_the_feature(self) -> None:
        self.write(FEATURES, member_features=["svg"])
        self.refuses("svg", workspaces=[self.workspace])

    def test_a_missing_feature_is_refused_naming_the_built_features(self) -> None:
        self.write(FEATURES[1:])
        self.refuses(*FEATURES, workspaces=[self.workspace])

    def test_a_member_glob_is_refused_naming_the_glob(self) -> None:
        self.write(FEATURES, member="crates/*")
        self.refuses("crates/*", workspaces=[self.workspace])


if __name__ == "__main__":
    unittest.main()
