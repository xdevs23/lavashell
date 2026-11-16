#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import contextlib
import dataclasses
import io
import json
import tempfile
import unittest
from collections.abc import Callable, Collection, Iterable
from dataclasses import asdict, dataclass, field, replace
from enum import Enum
from pathlib import Path
from unittest.mock import Mock, patch

import tomli_w

import cargo_plan as identities
import concurrent_checks
import dependency_workspace as projection
import refresh_cargo_projections as refresh
from cargo_files import CargoLock, LockedPackage, LockReference, read_toml
from cargo_models import (CargoMetadata, CrateType, DependencyKind, Edge, Package, PackageDependency, Profile,
                          TargetIdentity, TargetKind, Unit, UnitGraph, UnitMode)
from dependency_workspace import CanonicalPlan, OriginalContract, Selection, StubTarget
from document_tables import DocumentTable, TomlTable, array_at, string_at, table_at
from projection_contract import CONTRACT, Layer
from projection_files import LayerPlan, NativeProjection, ProjectionFile
from verification_errors import (ConflictingRequests, ExternalContextsChanged, ExternalPathPackage, LocalLinks,
                                 OriginalBuildScript, OriginalLinks, PathPatch, UnsupportedEdgeFlags,
                                 UnsupportedSource, VerificationError)

ROOT = "/fixture"
TARGET = "x86_64-unknown-linux-gnu"
NATIVE_PACKAGE = "app"


def package(name: str, local: bool = False, dependencies: tuple[PackageDependency, ...] = (),
            version: str = "1.0.0") -> Package:
    source = None if local else projection.CRATES_IO
    identity = f"path+file://{ROOT}/{name}#{name}@{version}" if local else f"{source}#{name}@{version}"
    return Package(id=identity, name=name, version=version, edition="2024", source=source,
                   manifest_path=Path(f"{ROOT}/{name}/Cargo.toml"), dependencies=dependencies, links=None)


def declaration(name: str, rename: str | None = None, kind: DependencyKind = DependencyKind.NORMAL,
                source: str | None = projection.CRATES_IO) -> PackageDependency:
    return PackageDependency(name=name, rename=rename, kind=kind, source=source)


def profile(host: bool) -> Profile:
    return Profile(name="packaged", opt_level="0" if host else "2", lto="false", codegen_backend=None,
                   codegen_units=None, debuginfo=0, debug_assertions=False, overflow_checks=False, rpath=False,
                   incremental=False, panic="unwind", split_debuginfo=None,
                   strip={"resolved": {"Named": "debuginfo"}})


def unit(package_: Package, dependencies: tuple[Edge, ...] = (), features: tuple[str, ...] = (), host: bool = False,
         kind: TargetKind = TargetKind.LIB, mode: UnitMode = UnitMode.BUILD) -> Unit:
    crate_type = CrateType.PROC_MACRO if kind is TargetKind.PROC_MACRO else CrateType.LIB
    return Unit(pkg_id=package_.id, target=TargetIdentity(package_.name.replace("-", "_"), (kind,), (crate_type,)),
                profile=profile(host), platform=None if host else TARGET, mode=mode, features=features,
                dependencies=dependencies)


def edge(index: int, name: str) -> Edge:
    return Edge(index=index, extern_crate_name=name.replace("-", "_"), noprelude=False, nounused=False, public=False)


def replaced[T](items: tuple[T, ...], index: int, item: T) -> tuple[T, ...]:
    return (*items[:index], item, *items[index + 1:])


def cargo_value(value: object) -> object:
    match value:
        case Enum():
            return value.value
        case Path():
            return str(value)
    raise TypeError(f"{type(value).__name__} is not part of the Cargo output")


def cargo_json(model: UnitGraph | CargoMetadata) -> str:
    return json.dumps(asdict(model), default=cargo_value)


def lock_package(name: str, version: str, source: str | None = None, dependencies: Iterable[str] = ()) -> LockedPackage:
    return LockedPackage(name, version, source, None, tuple(LockReference.parse(text) for text in dependencies))


@dataclass
class Fixture:
    graph: UnitGraph
    metadata: CargoMetadata
    selection: Selection
    manifest: TomlTable
    contracts: tuple[OriginalContract, ...] = ()

    @classmethod
    def default(cls) -> "Fixture":
        app = package("app", True, (declaration("dep"),))
        dep = package("dep")
        return cls(
            graph=UnitGraph(units=(unit(app, (edge(1, "dep"),)), unit(dep, features=("std", "default"))), roots=(0,)),
            metadata=CargoMetadata(workspace_root=Path(ROOT), packages=(app, dep)),
            selection=Selection(target=TARGET, profile="packaged", packages=("app",), features=()),
            manifest={"workspace": {"resolver": "3", "members": ["app"]},
                      "profile": {"packaged": {"inherits": "dev", "opt-level": 2}}},
        )

    def plan(self) -> CanonicalPlan:
        seed = projection.Seed(DocumentTable.decoded(self.manifest, "seed manifest"), self.contracts)
        return projection.canonical_plan(seed, self.graph, self.metadata, self.selection)

    def package(self, index: int) -> Package:
        return self.metadata.packages[index]

    def set_package(self, index: int, changed: Package) -> None:
        self.metadata = replace(self.metadata, packages=replaced(self.metadata.packages, index, changed))

    def add_packages(self, *added: Package) -> None:
        self.metadata = replace(self.metadata, packages=(*self.metadata.packages, *added))

    def add_declarations(self, index: int, *added: PackageDependency) -> None:
        declared = self.package(index)
        self.set_package(index, replace(declared, dependencies=(*declared.dependencies, *added)))

    def unit(self, index: int) -> Unit:
        return self.graph.units[index]

    def set_unit(self, index: int, changed: Unit) -> None:
        self.graph = replace(self.graph, units=replaced(self.graph.units, index, changed))

    def add_units(self, *added: Unit) -> None:
        self.graph = replace(self.graph, units=(*self.graph.units, *added))

    def add_edges(self, index: int, *added: Edge) -> None:
        self.set_unit(index, replace(self.unit(index), dependencies=(*self.unit(index).dependencies, *added)))

    def add_roots(self, *added: int) -> None:
        self.graph = replace(self.graph, roots=(*self.graph.roots, *added))


def seed_plan(lock: CargoLock) -> TomlTable:
    return {"manifest": tomli_w.dumps(Fixture.default().manifest), "lock": tomli_w.dumps(lock.to_toml()),
            "seedHash": "fixture",
            "crates": [{"member": "app", "manifest": tomli_w.dumps({"package": {"name": "app"}}),
                        "targets": [{"path": "src/lib.rs", "main": False}]}]}


def write_manifest(directory: Path, name: str) -> None:
    directory.mkdir(parents=True)
    (directory / "Cargo.toml").write_text(tomli_w.dumps({"package": {"name": name}}))


def write_example(directory: Path, name: str) -> None:
    write_manifest(directory, name)
    (directory / "src").mkdir()
    (directory / "src/main.rs").write_text("fn main() {}\n")


type NixEvaluation = Callable[[str, object], object]
type CargoRun = Callable[[list[str], Path, object], str]


def nix_for(*, on_catalog: Callable[[], None] = lambda: None, examples: Collection[str] = (),
            lock: CargoLock = CargoLock(version=4, packages=())) -> NixEvaluation:
    def evaluate(function: str, arguments: object) -> object:
        if function == refresh.CATALOG:
            on_catalog()
            return {"needsEngine": {example: False for example in examples}, "nativeNames": []}
        return seed_plan(lock)
    return evaluate


@dataclass
class CargoRuns:
    failing: Collection[str] = ()
    requested: list[str] = field(default_factory=list)

    def run(self, command: list[str], directory: Path, environment: object) -> str:
        fixture = Fixture.default()
        if command[1] == "metadata":
            return cargo_json(fixture.metadata)
        selected = command[command.index("--package") + 1]
        self.requested.append(selected)
        if selected in self.failing:
            raise SystemExit("cargo failed")
        return cargo_json(fixture.graph)


def run_refresh(root: Path, *arguments: str, nix: NixEvaluation | Mock, cargo: CargoRun | Mock) -> None:
    (root / "nix").mkdir()
    (root / "Cargo.lock").write_text(tomli_w.dumps(CargoLock(version=4, packages=()).to_toml()))
    write_manifest(root / CONTRACT.native_directory, NATIVE_PACKAGE)
    with patch.object(refresh, "CONTRACT", dataclasses.replace(CONTRACT, packaged_crates={})), \
            patch.object(refresh, "nix_eval", nix), patch.object(refresh, "run_cargo", cargo), \
            contextlib.redirect_stdout(io.StringIO()):
        refresh.main(list(arguments), root=root)


def stand_in(seed_hash: str) -> LayerPlan:
    return LayerPlan(seed_hash=seed_hash, plan=Fixture.default().plan())


class DependencyWorkspace(unittest.TestCase):
    def test_the_plan_keeps_the_registry_dependencies_that_cargo_reports_for_the_selected_target(self) -> None:
        fixture = Fixture.default()
        app, dependency = fixture.metadata.packages

        def cargo(command: list[str], directory: Path, environment: object) -> str:
            if command[1] != "metadata":
                return cargo_json(fixture.graph)
            platform = command[command.index("--filter-platform") + 1]
            reported = (app,) if "--no-deps" in command or platform != TARGET else (app, dependency)
            return cargo_json(replace(fixture.metadata, packages=reported))

        options = refresh.RefreshOptions(cargo="cargo", rustc="rustc", target=TARGET)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_manifest(root / "app", "app")
            with patch.object(refresh, "nix_eval", nix_for()), patch.object(refresh, "run_cargo", cargo):
                layer = refresh.dependency_plan(options, root, "app", Layer.REGISTRY, root / "result")
        self.assertEqual(string_at(layer.plan.crates[0].manifest, "dependencies", "dep", "version"), "=1.0.0")

    def test_layers_are_classified_from_the_native_lock_written_by_the_same_run(self) -> None:
        refreshed_lock = CargoLock(version=4, packages=(lock_package("added", "1.0.0", projection.CRATES_IO),))
        classified_with: list[CargoLock] = []
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            native = root / "nix/cargo-projections/native.json"
            nix = nix_for(on_catalog=lambda: classified_with.append(NativeProjection.read(native).lock),
                          lock=refreshed_lock)
            run_refresh(root, nix=nix, cargo=CargoRuns().run)
        self.assertEqual(classified_with, [refreshed_lock])

    def test_a_misspelled_package_is_rejected_before_the_refresh_writes_or_runs_cargo(self) -> None:
        cargo, nix = Mock(), Mock()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_example(root / "examples/demo", "demo")
            with contextlib.redirect_stderr(io.StringIO()) as stderr, self.assertRaises(SystemExit):
                run_refresh(root, "--package", "dmeo", nix=nix, cargo=cargo)
            self.assertFalse((root / "nix/cargo-projections").exists())
        self.assertIn("dmeo", stderr.getvalue())
        cargo.assert_not_called()
        nix.assert_not_called()

    def test_a_failed_package_is_named_and_the_packages_queued_behind_it_do_not_start(self) -> None:
        runs = CargoRuns(failing=("first",))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("first", "second"):
                write_example(root / "examples" / name, name)
            nix = nix_for(examples=("examples/first", "examples/second"))
            with patch.object(concurrent_checks, "concurrent_count", return_value=1), \
                    self.assertRaises(SystemExit) as failure:
                run_refresh(root, nix=nix, cargo=runs.run)
        self.assertIn("first", runs.requested)
        self.assertNotIn("second", runs.requested)
        self.assertEqual(failure.exception.__notes__, ["while refreshing the projection of first"])

    def test_engine_and_native_transitions_remove_obsolete_records_across_targets(self) -> None:
        empty = CargoLock(version=4, packages=())
        previous = ProjectionFile(
            normal_lock=empty, engine_lock=empty, features={}, external_units=[],
            dependency_plans={"current": {Layer.ENGINE: stand_in("old-engine"),
                                          Layer.REGISTRY: stand_in("old-registry")},
                              "other": {Layer.ENGINE: stand_in("other-engine"),
                                        Layer.REGISTRY: stand_in("other-registry")}})
        registry = previous.prepared(Layer.REGISTRY, "current")
        remaining = {"other": {Layer.REGISTRY: stand_in("other-registry")}}
        self.assertEqual(registry, ProjectionFile(normal_lock=empty, dependency_plans=remaining))
        engine = previous.prepared(Layer.ENGINE, "current")
        self.assertEqual(engine.dependency_plans, {"other": {Layer.ENGINE: stand_in("other-engine")}})
        self.assertEqual(engine.engine_lock, empty)
        self.assertEqual(previous.dependency_plans["current"][Layer.ENGINE], stand_in("old-engine"))

    def test_original_engine_contract_is_checked_after_cargo_selects_active_members(self) -> None:
        fixture = Fixture.default()
        fixture.contracts = (OriginalContract(member="inactive", links="native", build_script="build.rs"),)
        fixture.plan()
        for contract, cause in ((OriginalContract("app", "native", None), OriginalLinks(member="app")),
                                (OriginalContract("app", None, "build.rs"), OriginalBuildScript(member="app"))):
            fixture.contracts = (contract,)
            with self.assertRaises(VerificationError) as failure:
                fixture.plan()
            self.assertEqual(failure.exception.cause, cause)

    def test_a_build_script_and_its_local_build_time_crate_enter_the_plan(self) -> None:
        fixture = Fixture.default()
        app, dep = fixture.metadata.packages
        keys = package("keys", True, (declaration("dep"),))
        fixture.add_declarations(0, declaration("keys", kind=DependencyKind.BUILD, source=None))
        fixture.add_packages(keys)
        fixture.add_units(unit(dep, features=("host",), host=True), unit(keys, (edge(2, "dep"),), host=True),
                          unit(app, (edge(3, "keys"),), host=True, kind=TargetKind.CUSTOM_BUILD),
                          unit(app, (edge(4, "build_script_build"),), mode=UnitMode.RUN_CUSTOM_BUILD))
        fixture.add_edges(0, edge(5, "build_script_build"))
        fixture.contracts = (OriginalContract("app", None, "build.rs"), OriginalContract("keys", None, None))
        crates = {crate.member: crate for crate in fixture.plan().crates}
        self.assertEqual(string_at(crates["app"].manifest, "package", "build"), "build.rs")
        self.assertIn(StubTarget("build.rs", main=True), crates["app"].targets)
        self.assertEqual(string_at(crates["app"].manifest, "build-dependencies", "keys", "path"), "../keys")
        self.assertEqual(array_at(crates["keys"].manifest, "dependencies", "dep", "features"), ["host"])
        self.assertIs(table_at(crates["keys"].manifest, "package")["build"], False)

    def test_engine_seed_refreshes_local_closure_and_keeps_registry_jni_edges(self) -> None:
        registry = lock_package("jni", "0.22.4", projection.CRATES_IO)
        original = CargoLock(version=4, packages=(
            registry, lock_package("app", "0.1.0", dependencies=("native", "new-local", "jni")),
            lock_package("new-local", "0.1.0"), lock_package("native", "0.1.0")))
        baseline = CargoLock(version=4, packages=(
            lock_package("jni", "0.22.4", projection.CRATES_IO, ("registry-child",)), lock_package("app", "0.0.0"),
            lock_package("removed-local", "0.0.0")))
        result = refresh.engine_seed_lock(original, baseline, ["native"])
        self.assertEqual(result.packages[0], baseline.packages[0])
        app = next(locked for locked in result.packages if locked.name == "app")
        self.assertEqual([reference.name for reference in app.dependencies], ["new-local", "jni"])
        names = [locked.name for locked in result.packages]
        self.assertIn("new-local", names)
        self.assertNotIn("removed-local", names)

    def test_unused_declarations_and_local_closure_do_not_enter_plan(self) -> None:
        fixture = Fixture.default()
        before = fixture.plan()
        fixture.add_declarations(0, declaration("platform-only", source=None))
        fixture.add_packages(package("platform-only", True))
        array_at(fixture.manifest, "workspace", "members").append("platform-only")
        after = fixture.plan()
        self.assertEqual(before, after)
        self.assertEqual([crate.member for crate in after.crates], ["app"])

    def test_equivalent_requests_have_one_canonical_dependency(self) -> None:
        fixture = Fixture.default()
        before = fixture.plan()
        fixture.add_declarations(0, declaration("dep"))
        self.assertEqual(before, fixture.plan())
        dependency = table_at(before.crates[0].manifest, "dependencies", "dep")
        self.assertEqual(dependency, {"package": "dep", "version": "=1.0.0", "default-features": False,
                                      "features": ["default", "std"]})

    def test_registry_version_feature_profile_and_patch_changes_are_retained(self) -> None:
        fixture = Fixture.default()
        baseline = fixture.plan()
        dep_unit = fixture.unit(1)
        fixture.set_unit(1, replace(dep_unit, features=(*dep_unit.features, "changed")))
        self.assertNotEqual(baseline, fixture.plan())
        fixture.set_unit(1, dep_unit)
        dep = fixture.package(1)
        fixture.set_package(1, replace(dep, version="2.0.0"))
        self.assertNotEqual(baseline, fixture.plan())
        fixture.set_package(1, dep)
        table_at(fixture.manifest, "profile", "packaged")["opt-level"] = 3
        self.assertNotEqual(baseline, fixture.plan())
        fixture.manifest["patch"] = {"crates-io": {"patched": {"git": "https://example.invalid/repo"}}}
        self.assertEqual(fixture.plan().manifest["patch"], fixture.manifest["patch"])

    def test_aliases_and_two_exact_versions_stay_distinct(self) -> None:
        fixture = Fixture.default()
        fixture.set_package(0, replace(fixture.package(0), dependencies=(declaration("dep", "old"),
                                                                          declaration("dep", "new"))))
        new = package("dep", version="2.0.0")
        fixture.add_packages(new)
        fixture.add_units(unit(new))
        fixture.set_unit(0, replace(fixture.unit(0), dependencies=(edge(1, "old"), edge(2, "new"))))
        manifest = fixture.plan().crates[0].manifest
        self.assertEqual(string_at(manifest, "dependencies", "old", "version"), "=1.0.0")
        self.assertEqual(string_at(manifest, "dependencies", "new", "version"), "=2.0.0")

    def test_host_build_dependencies_and_normal_proc_macros_keep_edge_roles(self) -> None:
        fixture = Fixture.default()
        app, dep = fixture.metadata.packages
        fixture.add_declarations(0, declaration("dep", kind=DependencyKind.BUILD), declaration("derive"))
        derive = package("derive")
        fixture.add_packages(derive)
        fixture.add_units(unit(dep, features=("host",), host=True),
                          unit(derive, host=True, kind=TargetKind.PROC_MACRO),
                          unit(app, (edge(2, "dep"),), host=True, kind=TargetKind.CUSTOM_BUILD))
        fixture.add_edges(0, edge(3, "derive"), edge(4, "build_script_build"))
        manifest = fixture.plan().crates[0].manifest
        self.assertIn("derive", table_at(manifest, "dependencies"))
        self.assertEqual(array_at(manifest, "build-dependencies", "dep", "features"), ["host"])
        self.assertEqual(array_at(manifest, "dependencies", "dep", "features"), ["default", "std"])
        self.assertEqual(string_at(manifest, "package", "build"), "build.rs")

    def test_conflicting_requests_in_one_host_target_manifest_slot_fail(self) -> None:
        fixture = Fixture.default()
        app, dep = fixture.metadata.packages
        fixture.add_units(unit(dep, features=("host",), host=True), unit(app, (edge(2, "dep"),), host=True))
        fixture.add_roots(3)
        with self.assertRaises(VerificationError) as failure:
            fixture.plan()
        self.assertEqual(failure.exception.cause, ConflictingRequests(package="app", table="dependencies", alias="dep"))

    def test_no_temporary_absolute_paths_are_stored(self) -> None:
        fixture = Fixture.default()
        child = package("child", True)
        fixture.add_packages(child)
        fixture.add_declarations(0, declaration("child", source=None))
        fixture.add_units(unit(child))
        fixture.add_edges(0, edge(2, "child"))
        plan = fixture.plan()
        self.assertNotIn(ROOT, json.dumps(plan.to_json()))
        app = next(crate for crate in plan.crates if crate.member == "app")
        self.assertEqual(string_at(app.manifest, "dependencies", "child", "path"), "../child")

    def test_unreachable_units_are_excluded(self) -> None:
        fixture = Fixture.default()
        foreign = package("foreign", True)
        fixture.add_packages(foreign)
        fixture.add_units(unit(foreign))
        self.assertEqual(len(fixture.plan().crates), 1)

    def test_a_crates_io_dependency_patched_to_git_is_requested_by_its_exact_version(self) -> None:
        fixture = Fixture.default()
        git = "git+https://example.invalid/repo?rev=0123abc#0123abc"
        fixture.manifest["patch"] = {"crates-io": {"dep": {"git": "https://example.invalid/repo", "rev": "0123abc"}}}
        patched = replace(fixture.package(1), source=git, id=f"{git}#dep@1.0.0")
        fixture.set_package(1, patched)
        fixture.set_unit(1, replace(fixture.unit(1), pkg_id=patched.id))
        plan = fixture.plan()
        self.assertEqual(string_at(plan.crates[0].manifest, "dependencies", "dep", "version"), "=1.0.0")
        self.assertEqual(plan.manifest["patch"], fixture.manifest["patch"])
        self.assertEqual(len(identities.external_units(fixture.graph)), 1)

    def test_unsupported_source_and_build_contracts_fail_explicitly(self) -> None:
        git = "git+https://example.invalid/repo"

        def git_source(fixture: Fixture) -> None:
            app, dep = fixture.metadata.packages
            fixture.set_package(1, replace(dep, source=git))
            fixture.set_package(0, replace(app, dependencies=(replace(app.dependencies[0], source=git),)))

        def native_links(fixture: Fixture) -> None:
            fixture.set_package(0, replace(fixture.package(0), links="native-library"))

        def outside_path(fixture: Fixture) -> None:
            fixture.set_package(0, replace(fixture.package(0), manifest_path=Path("/outside/Cargo.toml")))

        app_id = Fixture.default().package(0).id
        for change, cause in ((git_source, UnsupportedSource(source=git)),
                              (native_links, LocalLinks(package="app")),
                              (outside_path, ExternalPathPackage(package=app_id))):
            fixture = Fixture.default()
            change(fixture)
            with self.assertRaises(VerificationError) as failure:
                fixture.plan()
            self.assertEqual(failure.exception.cause, cause)

    def test_a_materialized_plan_is_read_back_as_written(self) -> None:
        plan = Fixture.default().plan()
        lock = CargoLock(version=4, packages=(lock_package("app", "0.0.0", dependencies=("dep",)),))
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "plan"
            projection.materialize(plan, destination, lock)
            self.assertEqual(read_toml(destination / "Cargo.toml").toml_table(), plan.manifest)
            self.assertEqual(CargoLock.read(destination / "Cargo.lock"), lock)
            for crate in plan.crates:
                self.assertEqual(read_toml(destination / crate.member / "Cargo.toml").toml_table(), crate.manifest)

    def test_native_common_keeps_explicit_renderer_selection_and_active_closure(self) -> None:
        fixture = Fixture.default()
        renderer = package("lavashell-renderer", True)
        fixture.add_packages(renderer, package("unused-platform", True))
        fixture.add_declarations(0, declaration("lavashell-renderer", source=None),
                                 declaration("unused-platform", source=None))
        fixture.add_units(unit(renderer, features=("selected",)))
        fixture.add_edges(0, edge(2, "lavashell-renderer"))
        fixture.add_roots(2)
        fixture.selection = replace(fixture.selection, packages=(*fixture.selection.packages, "lavashell-renderer"),
                                    features=(*fixture.selection.features, "lavashell-renderer/selected"))
        plan = fixture.plan()
        self.assertEqual(plan.selection, fixture.selection)
        self.assertEqual(array_at(plan.manifest, "workspace", "members"), ["app", "lavashell-renderer"])
        self.assertEqual(table_at(plan.crates[1].manifest, "features"), {"selected": []})

    def test_path_patch_and_dependency_edge_flags_are_explicit_failures(self) -> None:
        fixture = Fixture.default()
        fixture.manifest["patch"] = {"crates-io": {"dep": {"path": "patched"}}}
        with self.assertRaises(VerificationError) as patched:
            fixture.plan()
        self.assertEqual(patched.exception.cause, PathPatch())
        del fixture.manifest["patch"]
        app_unit = fixture.unit(0)
        fixture.set_unit(0, replace(app_unit, dependencies=(replace(app_unit.dependencies[0], public=True),)))
        with self.assertRaises(VerificationError) as flagged:
            fixture.plan()
        self.assertEqual(flagged.exception.cause, UnsupportedEdgeFlags(parent="app", child="dep"))

    def test_frozen_context_verification_compares_full_records_not_counts(self) -> None:
        fixture = Fixture.default()
        changed = replace(fixture.graph, units=replaced(fixture.graph.units, 1,
                                                        replace(fixture.unit(1), features=("other",))))
        with self.assertRaises(VerificationError) as failure:
            projection.verify_units(fixture.graph, changed)
        self.assertEqual(failure.exception.cause, ExternalContextsChanged(missing=1, extra=1))
        projection.verify_units(fixture.graph, Fixture.default().graph)


if __name__ == "__main__":
    unittest.main()
