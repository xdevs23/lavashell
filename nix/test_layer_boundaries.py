#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import shutil
import tempfile
import unittest
from collections.abc import Callable, Sequence
from dataclasses import dataclass, replace
from enum import Enum
from pathlib import Path

import tomli_w

from cargo_files import CargoLock, LockedPackage, LockReference, read_toml
from document_tables import DocumentTable, TomlTable, array_at, string_at, table_at
from nix_evaluation import NixEvaluationError, nix_eval
from projection_contract import CONTRACT
from repository_paths import REPOSITORY_ROOT

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


class Derivation(Enum):
    REGISTRY = "registry"
    FRAMEWORK = "framework"
    ENGINE = "engine"
    REFERENCE_FRAMEWORK = "referenceFramework"
    REFERENCE = "reference"
    NATIVE = "native"


LAYERS = tuple(Derivation)
LAYERS_ABOVE_ENGINE = tuple(layer for layer in Derivation if layer is not Derivation.ENGINE)
LAYERS_ON_ENGINE = (Derivation.ENGINE, Derivation.REFERENCE_FRAMEWORK, Derivation.REFERENCE)


class Variant(Enum):
    PLAIN = "plain"
    GUI_TOOLS = "gui-tools"


DERIVATIONS = """
  { nixpkgs, root, variant, keys }:
  let
    original = import nixpkgs { };
    renamed = package: package.overrideAttrs (old: { pname = old.pname + "-fixture"; });
    pkgs =
      if variant == "gui-tools" then
        original
        // {
          fontconfig = renamed original.fontconfig;
          freetype = renamed original.freetype;
          clang = renamed original.clang;
        }
      else
        original;
    tree = (import ./examples-tree.nix { inherit pkgs; root = /. + root; }).examples;
    reference = tree.reference;
  in
  original.lib.getAttrs keys {
    registry = tree.checks.registry.drvPath;
    framework = tree.checks.framework.drvPath;
    engine = reference.engineArtifacts.drvPath;
    referenceFramework = reference.frameworkArtifacts.drvPath;
    reference = reference.drvPath;
    native = tree.packages.interactions.drvPath;
    directory = toString reference.directory;
  }
"""
DIRECTORY = "directory"


@dataclass(frozen=True)
class DerivationArguments:
    root: Path
    variant: str
    keys: tuple[str, ...]


def evaluated(root: Path, variant: Variant, keys: tuple[str, ...]) -> DocumentTable:
    return DocumentTable.decoded(nix_eval(DERIVATIONS, DerivationArguments(root, variant.value, keys)),
                                 "layer derivations")


def evaluate(root: Path, variant: Variant = Variant.PLAIN,
             keys: Sequence[Derivation] = LAYERS) -> dict[Derivation, str]:
    derivations = evaluated(root, variant, tuple(key.value for key in keys))
    return {key: derivations.string(key.value) for key in keys}


def reference_directory(root: Path) -> Path:
    return Path(evaluated(root, Variant.PLAIN, (DIRECTORY,)).string(DIRECTORY))


def copy_repository(destination: Path) -> None:
    destination.mkdir()
    for name in ("Cargo.toml", "Cargo.lock", "COPYING", "about.toml"):
        shutil.copyfile(REPOSITORY_ROOT / name, destination / name)
    sources = [CONTRACT.projection_directory, "crates", "examples", "tools"]
    for name in sources:
        shutil.copytree(REPOSITORY_ROOT / name, destination / name, copy_function=shutil.copyfile,
                        ignore=shutil.ignore_patterns("node_modules", "target"))
    for directory in [destination, *(path for path in destination.rglob("*") if path.is_dir())]:
        directory.chmod(0o755)


def edit_manifest(path: Path, update: Callable[[TomlTable], None]) -> None:
    manifest = read_toml(path).toml_table()
    update(manifest)
    path.write_text(tomli_w.dumps(manifest))


def edit_lock(path: Path, update: Callable[[CargoLock], CargoLock]) -> None:
    path.write_text(tomli_w.dumps(update(CargoLock.read(path)).to_toml()))


def bumped(version: str) -> str:
    *leading, patch = version.split(".")
    return ".".join([*leading, str(int(patch) + 1)])


def bump_locked_package(lock: CargoLock, name: str, old: str, new: str) -> CargoLock:
    matches = [package for package in lock.packages if (package.name, package.version) == (name, old)]
    if len(matches) != 1:
        raise LookupError(f"the lock contains {len(matches)} packages {name} {old}")
    return replace(lock, packages=tuple(replace(package, version=new) if package in matches else package
                                        for package in lock.packages))


def bump_workspace_dependency(fixture: Path, name: str) -> None:
    manifest = read_toml(fixture / "Cargo.toml").toml_table()
    old = string_at(manifest, "workspace", "dependencies", name, "version").removeprefix("=")

    def bump(manifest: TomlTable) -> None:
        table_at(manifest, "workspace", "dependencies", name)["version"] = "=" + bumped(old)

    edit_manifest(fixture / "Cargo.toml", bump)
    edit_lock(fixture / "Cargo.lock", lambda lock: bump_locked_package(lock, name, old, bumped(old)))


def reference_source(directory: Path) -> Path:
    sources = directory / "src"
    return sources / "main.rs" if (sources / "main.rs").exists() else sources / "lib.rs"


def with_foreign_packages(lock: CargoLock) -> CargoLock:
    foreign = (
        LockedPackage("foreign-fixture", "0.1.0", None, None, (LockReference("foreign-registry-fixture", None, None),)),
        LockedPackage("foreign-registry-fixture", "1.0.0", CRATES_IO, "0" * 64, ()),
    )
    return replace(lock, packages=(*lock.packages, *foreign))


class LayerBoundaryTests(unittest.TestCase):
    directory: Path
    repository: Path
    base: dict[Derivation, str]
    reference: Path

    @classmethod
    def setUpClass(cls) -> None:
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.directory = Path(temporary.name)
        cls.repository = cls.directory / "repository"
        copy_repository(cls.repository)
        cls.base = evaluate(cls.repository)
        cls.reference = reference_directory(cls.repository).relative_to(cls.repository)

    def fixture(self, label: str) -> Path:
        fixture = self.directory / label
        copy_repository(fixture)
        return fixture

    def assert_unchanged(self, current: dict[Derivation, str], layers: Sequence[Derivation]) -> None:
        for layer in layers:
            self.assertEqual(current[layer], self.base[layer], f"{layer.value} changed")

    def assert_changed(self, current: dict[Derivation, str], layers: Sequence[Derivation]) -> None:
        for layer in layers:
            self.assertNotEqual(current[layer], self.base[layer], f"{layer.value} did not change")

    def stale_layers(self, fixture: Path) -> tuple[Derivation, ...]:
        stale = []
        for layer in LAYERS:
            try:
                evaluate(fixture, keys=[layer])
            except NixEvaluationError:
                stale.append(layer)
        return tuple(stale)

    def test_changed_gui_tools_do_not_reach_the_engine(self) -> None:
        tools = evaluate(self.repository, Variant.GUI_TOOLS)
        self.assert_unchanged(tools, [Derivation.ENGINE])
        self.assert_changed(tools, LAYERS_ABOVE_ENGINE)

    def test_an_edit_of_the_reference_source_reaches_only_the_reference(self) -> None:
        fixture = self.fixture("reference_source")
        source = reference_source(fixture / self.reference)
        source.write_text(source.read_text() + "\n")
        current = evaluate(fixture)
        self.assert_unchanged(current, [layer for layer in LAYERS if layer is not Derivation.REFERENCE])
        self.assert_changed(current, [Derivation.REFERENCE])

    def test_an_edit_of_the_framework_source_leaves_the_registry_and_the_engine(self) -> None:
        fixture = self.fixture("framework_source")
        source = fixture / "crates/lavashell-core/src/lib.rs"
        source.write_text(source.read_text() + "\n")
        current = evaluate(fixture)
        self.assert_unchanged(current, [Derivation.REGISTRY, Derivation.ENGINE])
        self.assert_changed(current, [Derivation.FRAMEWORK, Derivation.REFERENCE_FRAMEWORK, Derivation.REFERENCE,
                                      Derivation.NATIVE])

    def test_an_unrelated_example_and_its_registry_update_reach_no_layer(self) -> None:
        fixture = self.fixture("unrelated_example")
        foreign = fixture / "examples/foreign-fixture"
        (foreign / "src").mkdir(parents=True)
        (foreign / "src/main.rs").write_text("fn main() {}\n")
        foreign_manifest: TomlTable = {
            "package": {"name": "foreign-fixture", "version": "0.1.0", "edition": "2024"},
            "dependencies": {"foreign-registry-fixture": "=1.0.0"},
        }
        (foreign / "Cargo.toml").write_text(tomli_w.dumps(foreign_manifest))
        edit_manifest(fixture / "Cargo.toml",
                      lambda manifest: array_at(manifest, "workspace", "members").append("examples/foreign-fixture"))
        edit_lock(fixture / "Cargo.lock", with_foreign_packages)
        self.assertEqual(evaluate(fixture), self.base)
        edit_lock(fixture / "Cargo.lock",
                  lambda lock: bump_locked_package(lock, "foreign-registry-fixture", "1.0.0", "1.0.1"))
        self.assertEqual(evaluate(fixture), self.base)

    def test_a_registry_update_of_gui_crates_makes_the_projections_stale_and_leaves_the_engine(self) -> None:
        fixture = self.fixture("gui_registry")
        bump_workspace_dependency(fixture, "zbus")
        self.assertEqual(self.stale_layers(fixture), LAYERS_ABOVE_ENGINE)
        self.assert_unchanged(evaluate(fixture, keys=[Derivation.ENGINE]), [Derivation.ENGINE])

    def test_an_added_engine_feature_makes_the_projections_stale_and_leaves_the_other_layers(self) -> None:
        fixture = self.fixture("engine_features")
        edit_manifest(fixture / self.reference / "Cargo.toml",
                      lambda manifest: array_at(manifest, "dependencies", "bevy", "features").append("bevy_scene"))
        self.assertEqual(self.stale_layers(fixture), LAYERS_ON_ENGINE)
        unaffected = [Derivation.REGISTRY, Derivation.FRAMEWORK, Derivation.NATIVE]
        self.assert_unchanged(evaluate(fixture, keys=unaffected), unaffected)

    def test_a_version_change_of_the_adapter_reaches_only_the_reference_layers(self) -> None:
        fixture = self.fixture("adapter_version")
        old = string_at(read_toml(fixture / "Cargo.toml").toml_table(), "workspace", "package", "version")

        def bump(manifest: TomlTable) -> None:
            table_at(manifest, "package")["version"] = bumped(old)

        edit_manifest(fixture / self.reference / "Cargo.toml", bump)
        edit_lock(fixture / "Cargo.lock", lambda lock: bump_locked_package(lock, "lavashell-bevy", old, bumped(old)))
        current = evaluate(fixture)
        self.assert_unchanged(current, [Derivation.REGISTRY, Derivation.FRAMEWORK, Derivation.ENGINE,
                                        Derivation.NATIVE])
        self.assert_changed(current, [Derivation.REFERENCE_FRAMEWORK, Derivation.REFERENCE])

    def test_a_version_change_of_the_engine_makes_the_projections_stale_and_leaves_the_other_layers(self) -> None:
        fixture = self.fixture("engine_version")
        bump_workspace_dependency(fixture, "bevy")
        self.assertEqual(self.stale_layers(fixture), LAYERS_ON_ENGINE)
        unaffected = [Derivation.REGISTRY, Derivation.FRAMEWORK, Derivation.NATIVE]
        self.assert_unchanged(evaluate(fixture, keys=unaffected), unaffected)


if __name__ == "__main__":
    unittest.main()
