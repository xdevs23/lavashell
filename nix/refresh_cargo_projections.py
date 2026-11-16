#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import argparse
import copy
import itertools
import os
import tempfile
import tomllib
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, replace
from functools import partial
from pathlib import Path
from typing import Self

import tomli_w

import cargo_plan
import dependency_workspace
from cargo_files import CargoLock, FeatureActivation, FeatureActivations, LockedPackage, package_name
from cargo_models import CargoMetadata, DependencyKind, PackageId, TargetKind, Unit, UnitGraph
from checked_commands import run_checked
from concurrent_checks import run_concurrently
from dependency_workspace import OriginalContract, Seed, Selection, StubTarget
from document_tables import DocumentTable
from nix_evaluation import nix_eval
from projection_contract import CONTRACT, Layer
from projection_files import LayerPlan, NativeProjection, ProjectionFile
from repository_paths import REPOSITORY_ROOT
from verification_errors import AmbiguousFeatureContext, CargoFailed, SharedFeaturesUnchanged, VerificationError


CATALOG = """
  { nixpkgs, root, directories }:
  let
    checkout = import ./checkout-sources.nix { inherit nixpkgs root; };
    inherit (checkout) sources directory;
  in
  {
    needsEngine = builtins.listToAttrs (map (relative: {
      name = relative;
      value = sources.needsEngine (directory relative);
    }) directories);
    inherit (sources) nativeNames;
  }
"""

SEED_PLAN = """
  { nixpkgs, root, relative, layer }:
  let
    checkout = import ./checkout-sources.nix { inherit nixpkgs root; };
  in
  checkout.sources.seedPlan (checkout.directory relative) layer
"""


@dataclass(frozen=True)
class CatalogArguments:
    root: Path
    directories: tuple[str, ...]


@dataclass(frozen=True)
class SeedArguments:
    root: Path
    relative: str
    layer: str


@dataclass(frozen=True)
class LayerCatalog:
    needs_engine: dict[str, bool]
    native_names: tuple[str, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        needs_engine = raw.object("needsEngine")
        return cls(needs_engine={directory: needs_engine.boolean(directory) for directory in needs_engine.keys()},
                   native_names=raw.strings("nativeNames"))


@dataclass(frozen=True)
class RefreshOptions:
    cargo: str
    rustc: str
    target: str


@dataclass(frozen=True)
class ProjectedCrate:
    member: str
    manifest: str
    targets: tuple[StubTarget, ...]


@dataclass(frozen=True)
class ProjectedWorkspace:
    manifest: str
    lock: str
    crates: tuple[ProjectedCrate, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            manifest=raw.string("manifest"),
            lock=raw.string("lock"),
            crates=tuple(
                ProjectedCrate(crate.string("member"), crate.string("manifest"),
                               tuple(StubTarget.from_json(target) for target in crate.objects("targets")))
                for crate in raw.objects("crates")),
        )


@dataclass(frozen=True)
class SeedWorkspace(ProjectedWorkspace):
    seed_hash: str
    original_contracts: tuple[OriginalContract, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        projected = ProjectedWorkspace.from_json(raw)
        contracts = raw.objects("originalContracts") if raw.has("originalContracts") else ()
        return cls(
            manifest=projected.manifest,
            lock=projected.lock,
            crates=projected.crates,
            seed_hash=raw.string("seedHash"),
            original_contracts=tuple(OriginalContract.from_json(contract) for contract in contracts),
        )

    def manifest_table(self) -> DocumentTable:
        return DocumentTable.decoded(tomllib.loads(self.manifest), "seed manifest")


@dataclass(frozen=True)
class PackageEntry:
    directory: str
    package: str
    layer: Layer


def classify_layers(root: Path, names: Mapping[str, str]) -> tuple[list[PackageEntry], tuple[str, ...]]:
    catalog = LayerCatalog.from_json(DocumentTable.decoded(nix_eval(CATALOG, CatalogArguments(root, tuple(names))),
                                                           "layer catalog"))
    return [PackageEntry(directory, package, Layer.ENGINE if catalog.needs_engine[directory] else Layer.REGISTRY)
            for directory, package in names.items()], catalog.native_names


def write_workspace[W: ProjectedWorkspace](workspace: W, destination: Path) -> W:
    destination.mkdir(parents=True)
    (destination / "Cargo.toml").write_text(workspace.manifest)
    (destination / "Cargo.lock").write_text(workspace.lock)
    for crate in workspace.crates:
        member = destination / crate.member
        member.mkdir(parents=True)
        (member / "Cargo.toml").write_text(crate.manifest)
        dependency_workspace.write_stub_targets(member, crate.targets)
    return workspace


def seed_workspace(root: Path, directory: str, layer: Layer, destination: Path) -> SeedWorkspace:
    seed = nix_eval(SEED_PLAN, SeedArguments(root, directory, layer.value))
    return write_workspace(SeedWorkspace.from_json(DocumentTable.decoded(seed, "seed plan")), destination)


def run_cargo(command: list[str], directory: Path, environment: Mapping[str, str]) -> str:
    return run_checked(command, CargoFailed, cwd=directory, env=environment)


def cargo_graph(args: RefreshOptions, directory: Path, package: str, companions: Iterable[str],
                frozen: bool = False) -> tuple[UnitGraph, CargoLock]:
    command = [args.cargo, "build", "--unit-graph", "-Z", "unstable-options", "--offline",
               "--profile", CONTRACT.profile, "--target", args.target, "--package", package]
    if frozen:
        command.append("--frozen")
    for companion in companions:
        command += ["--package", companion]
    environment = os.environ | {"RUSTC_BOOTSTRAP": "1", "RUSTC": args.rustc,
                                "CARGO_TARGET_DIR": str(directory / "target")}
    stdout = run_cargo(command, directory, environment)
    return UnitGraph.parse(stdout), CargoLock.read(directory / "Cargo.lock")


def companions_in(workspace: ProjectedWorkspace) -> tuple[str, ...]:
    names = {DocumentTable.decoded(tomllib.loads(crate.manifest), crate.member).object("package").string("name")
             for crate in workspace.crates}
    return tuple(companion for companion in CONTRACT.companions if companion in names)


def dependency_plan(args: RefreshOptions, root: Path, directory: str, layer: Layer, destination: Path) -> LayerPlan:
    seed_path = destination / "seed"
    seed = seed_workspace(root, directory, layer, seed_path)
    package = package_name(root / directory)
    companions = companions_in(seed)
    expected, seed_lock = cargo_graph(args, seed_path, package, companions, frozen=True)
    metadata = CargoMetadata.parse(run_cargo(
        [args.cargo, "metadata", "--offline", "--frozen", "--format-version", "1", "--filter-platform", args.target],
        seed_path, os.environ | {"RUSTC": args.rustc}))
    selection = Selection(target=args.target, profile=CONTRACT.profile, packages=(package, *companions), features=())
    canonical = dependency_workspace.canonical_plan(
        Seed(seed.manifest_table(), seed.original_contracts), expected, metadata, selection)
    canonical_path = destination / "canonical"
    dependency_workspace.materialize(canonical, canonical_path, seed_lock)
    actual, lock = cargo_graph(args, canonical_path, package, companions)
    dependency_workspace.verify_units(expected, actual)
    (canonical_path / "Cargo.lock").write_text(tomli_w.dumps(lock.to_toml()))
    frozen, _ = cargo_graph(args, canonical_path, package, companions, frozen=True)
    dependency_workspace.verify_units(expected, frozen)
    units = tuple(cargo_plan.external_units(frozen))
    return LayerPlan(seed.seed_hash, replace(canonical, lock=lock, external_units=units))


def engine_seed_lock(original: CargoLock, baseline: CargoLock, native_names: Sequence[str]) -> CargoLock:
    registry = [package for package in baseline.packages if package.source is not None]
    local = [without_native_dependencies(package, native_names)
             for package in original.packages if package.source is None]
    return replace(baseline, packages=(*registry, *local))


def without_native_dependencies(package: LockedPackage, native_names: Sequence[str]) -> LockedPackage:
    return replace(package, dependencies=tuple(reference for reference in package.dependencies
                                               if reference.name not in native_names))


def activations(engine: UnitGraph, consumer: UnitGraph) -> FeatureActivations:
    def identity(unit: Unit) -> str:
        return replace(unit.identity, features=()).text()
    candidates: dict[str, list[Unit]] = {}
    for unit in consumer.units:
        candidates.setdefault(identity(unit), []).append(unit)
    result: FeatureActivations = {}
    for unit in engine.units:
        if not cargo_plan.is_external(unit) or unit.target.kind != (TargetKind.LIB,):
            continue
        matches = candidates.get(identity(unit), [])
        if any(match.features == unit.features for match in matches):
            continue
        if len(matches) != 1:
            raise VerificationError(AmbiguousFeatureContext(package=unit.pkg_id))
        package = PackageId.parse(unit.pkg_id)
        kind = DependencyKind.BUILD if unit.platform is None else DependencyKind.NORMAL
        result.setdefault(kind, {})[package.name] = FeatureActivation.exact(package.version, matches[0].features)
    return result


@dataclass(frozen=True)
class RefreshRun:
    args: RefreshOptions
    root: Path
    original: CargoLock
    native_names: tuple[str, ...]
    output: Path
    scratch: Path

    def projection_path(self, entry: PackageEntry) -> Path:
        return self.output / f"{entry.package}.json"

    def normal_graph(self, entry: PackageEntry) -> tuple[UnitGraph, CargoLock]:
        normal = self.scratch / entry.package / "normal"
        workspace = seed_workspace(self.root, entry.directory, Layer.REGISTRY, normal)
        return cargo_graph(self.args, normal, entry.package, companions_in(workspace))

    def store_plan(self, entry: PackageEntry, projection: ProjectionFile) -> None:
        scratch = self.scratch / entry.package / f"{entry.layer.value}-layer"
        plan = dependency_plan(self.args, self.root, entry.directory, entry.layer, scratch)
        projection.dependency_plans.setdefault(self.args.target, {})[entry.layer] = plan
        projection.write(self.projection_path(entry))


def refresh_registry_layer(run: RefreshRun, entry: PackageEntry, projection: ProjectionFile) -> int | None:
    _, projection.normal_lock = run.normal_graph(entry)
    projection.write(run.projection_path(entry))
    run.store_plan(entry, projection)
    return None


def refresh_engine_layer(run: RefreshRun, entry: PackageEntry, projection: ProjectionFile) -> int | None:
    features: FeatureActivations = {}
    projection.features = features
    baseline = run.original if projection.engine_lock is None else projection.engine_lock
    projection.engine_lock = engine_seed_lock(run.original, baseline, run.native_names)
    projection.write(run.projection_path(entry))
    normal_graph, projection.normal_lock = run.normal_graph(entry)
    projection.write(run.projection_path(entry))
    attempts = itertools.count()
    while True:
        engine = run.scratch / entry.package / f"engine-{next(attempts)}"
        seed_workspace(run.root, entry.directory, Layer.ENGINE, engine)
        engine_graph, projection.engine_lock = cargo_graph(run.args, engine, entry.package, ())
        extra = activations(engine_graph, normal_graph)
        if not extra:
            projection.external_units = cargo_plan.external_units(engine_graph)
            cargo_plan.verify(projection.external_units, normal_graph)
            projection.write(run.projection_path(entry))
            run.store_plan(entry, projection)
            return len(projection.external_units)
        before = copy.deepcopy(features)
        for kind, dependencies in extra.items():
            features.setdefault(kind, {}).update(dependencies)
        if features == before:
            raise VerificationError(SharedFeaturesUnchanged(package=entry.package))
        projection.write(run.projection_path(entry))


def refresh_layer(run: RefreshRun, entry: PackageEntry) -> int | None:
    path = run.projection_path(entry)
    projection = ProjectionFile.read(path) if path.exists() else ProjectionFile(normal_lock=run.original)
    prepared = projection.prepared(entry.layer, run.args.target)
    try:
        return (refresh_engine_layer if entry.layer is Layer.ENGINE else refresh_registry_layer)(run, entry, prepared)
    except BaseException as failure:
        failure.add_note(f"while refreshing the projection of {entry.package}")
        raise


def package_names(root: Path, packaged_crates: Iterable[str]) -> dict[str, str]:
    examples = [f"examples/{path.parent.parent.name}" for path in sorted(root.glob("examples/*/src/main.rs"))]
    return {path: package_name(root / path) for path in examples + list(packaged_crates)}


def refresh_projections(args: RefreshOptions, root: Path, names: Mapping[str, str]) -> None:
    original = CargoLock.read(root / "Cargo.lock")
    output = root / CONTRACT.projection_directory
    output.mkdir(exist_ok=True)
    native_path = output / "native.json"
    native = NativeProjection.read(native_path) if native_path.exists() else NativeProjection(lock=original)
    native_package = package_name(root / CONTRACT.native_directory)
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        native_directory = directory / "native"
        native_workspace = seed_workspace(root, CONTRACT.native_directory, Layer.REGISTRY, native_directory)
        _, native_lock = cargo_graph(args, native_directory, native_package, companions_in(native_workspace))
        native = native.prepared(native_lock, args.target)
        native.write(native_path)
        catalog, native_names = classify_layers(root, names)
        native.dependency_plans.setdefault(args.target, {})[Layer.REGISTRY] = dependency_plan(
            args, root, CONTRACT.native_directory, Layer.REGISTRY, directory / "native-layer")
        native.write(native_path)
        run = RefreshRun(args=args, root=root, original=original, native_names=native_names, output=output,
                         scratch=directory)
        refreshed = run_concurrently([partial(refresh_layer, run, entry) for entry in catalog])
        for entry, units in zip(catalog, refreshed):
            if units is not None:
                print(f"{entry.package}: {units} matching external units")
    print("Projection locks refreshed; no compilation performed")


def main(argv: Sequence[str] | None = None, root: Path = REPOSITORY_ROOT) -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--target", default=CONTRACT.target)
    parser.add_argument("--package", action="append", help="Refresh selected example packages plus the native base")
    args = parser.parse_args(argv)
    selected: list[str] | None = args.package
    names = package_names(root, CONTRACT.packaged_crates.values())
    if selected is not None:
        native_package = package_name(root / CONTRACT.native_directory)
        unknown = set(selected) - set(names.values()) - {native_package}
        if unknown:
            parser.error(f"unknown packages: {sorted(unknown)}")
        names = {path: name for path, name in names.items() if name in selected}
    refresh_projections(RefreshOptions(cargo=args.cargo, rustc=args.rustc, target=args.target), root, names)


if __name__ == "__main__":
    main()
