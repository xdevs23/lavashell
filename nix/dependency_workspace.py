# SPDX-License-Identifier: GPL-3.0-only

import copy
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Self

import tomli_w

import cargo_plan
from cargo_files import CargoLock, DependencyRequest, PathRequest, StubManifest, VersionRequest
from cargo_models import CargoMetadata, DependencyKind, Edge, Package, TargetKind, Unit, UnitGraph, UnitMode
from document_tables import DocumentTable, TomlTable, TomlValue
from projection_contract import CONTRACT
from verification_errors import (AmbiguousAlias, ConflictingRequests, ExternalContextsChanged, ExternalDependsOnLocal,
                                 ExternalPathPackage, InvalidUnitIndex, LocalLinks, NonVirtualWorkspace,
                                 NoUnitsSelected, OriginalBuildScript, OriginalLinks, PackageIdentityMismatch,
                                 PathPatch, RootPackageMissing, SharedMember, UnsupportedEdgeFlags, UnsupportedMode,
                                 UnsupportedSource, UnsupportedTargetKind, VerificationError)

# https://doc.rust-lang.org/cargo/reference/registry-index.html
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SUPPORTED_LOCAL_KINDS = frozenset(
    {TargetKind.LIB, TargetKind.BIN, TargetKind.RLIB, TargetKind.CDYLIB, TargetKind.STATICLIB})
# https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions
DEFAULT_RESOLVER = "1"
COPIED_SECTIONS = ("profile", "patch", "replace")
PATCH_SECTIONS = ("patch", "replace")


@dataclass(frozen=True)
class Selection:
    target: str
    profile: str
    packages: tuple[str, ...]
    features: tuple[str, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(raw.string("target"), raw.string("profile"), raw.strings("packages"), raw.strings("features"))

    def to_json(self) -> TomlTable:
        return {"target": self.target, "profile": self.profile, "packages": list(self.packages),
                "features": list(self.features)}


@dataclass(frozen=True)
class OriginalContract:
    member: str
    links: str | None
    build_script: str | None

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(raw.string("member"), raw.optional_string("links"), raw.string_unless_false("build"))


@dataclass(frozen=True)
class Seed:
    workspace_manifest: DocumentTable
    original_contracts: tuple[OriginalContract, ...]


@dataclass(frozen=True)
class StubTarget:
    path: str
    main: bool

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(raw.string("path"), raw.boolean("main"))

    def to_json(self) -> TomlTable:
        return {"path": self.path, "main": self.main}


@dataclass(frozen=True)
class CanonicalCrate:
    member: str
    manifest: TomlTable
    targets: tuple[StubTarget, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(raw.string("member"), raw.object("manifest").toml_table(),
                   tuple(StubTarget.from_json(target) for target in raw.objects("targets")))

    def to_json(self) -> TomlTable:
        return {"member": self.member, "manifest": self.manifest,
                "targets": [target.to_json() for target in self.targets]}


@dataclass(frozen=True)
class CanonicalPlan:
    schema: int
    selection: Selection
    manifest: TomlTable
    crates: tuple[CanonicalCrate, ...]
    lock: CargoLock | None = None
    external_units: tuple[str, ...] | None = None

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        lock = raw.optional_object("lock")
        return cls(
            schema=raw.integer("schema"),
            selection=Selection.from_json(raw.object("selection")),
            manifest=raw.object("manifest").toml_table(),
            crates=tuple(CanonicalCrate.from_json(crate) for crate in raw.objects("crates")),
            lock=None if lock is None else CargoLock.from_table(lock),
            external_units=raw.strings("externalUnits") if raw.has("externalUnits") else None,
        )

    def to_json(self) -> TomlTable:
        raw: TomlTable = {"schema": self.schema, "selection": self.selection.to_json(), "manifest": self.manifest,
                          "crates": [crate.to_json() for crate in self.crates]}
        if self.lock is not None:
            raw["lock"] = self.lock.to_toml()
        if self.external_units is not None:
            raw["externalUnits"] = list(self.external_units)
        return raw


def selected_units(graph: UnitGraph) -> list[Unit]:
    pending = list(graph.roots)
    seen: set[int] = set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        if not 0 <= index < len(graph.units):
            raise VerificationError(InvalidUnitIndex(index=index))
        seen.add(index)
        pending.extend(edge.index for edge in graph.units[index].dependencies)
    if not seen:
        raise VerificationError(NoUnitsSelected())
    return [graph.units[index] for index in sorted(seen)]


def member_path(package: Package, workspace_root: Path) -> str:
    try:
        member = package.manifest_path.parent.relative_to(workspace_root).as_posix()
    except ValueError as error:
        raise VerificationError(ExternalPathPackage(package=package.id)) from error
    if member in ("", ".") or ".." in Path(member).parts:
        raise VerificationError(NonVirtualWorkspace(member=member))
    return member


def declared_alias(parent: Package, child: Package, unit: Unit, edge: Edge,
                   kind: DependencyKind) -> tuple[str, str | None]:
    candidates = set()
    for dependency in parent.dependencies:
        if dependency.name != child.name or dependency.kind is not kind:
            continue
        alias = dependency.rename or dependency.name
        expected = (dependency.rename or unit.target.name).replace("-", "_")
        if expected == edge.extern_crate_name:
            candidates.add((alias, dependency.source))
    if len(candidates) != 1:
        raise VerificationError(AmbiguousAlias(crate_name=edge.extern_crate_name, package=parent.name))
    return candidates.pop()


def dependency_request(parent: Package, child: Package, child_unit: Unit, edge: Edge, kind: DependencyKind,
                       members: dict[str, str]) -> tuple[str, DependencyRequest]:
    if edge.has_flags:
        raise VerificationError(UnsupportedEdgeFlags(parent=parent.name, child=child.name))
    alias, declared = declared_alias(parent, child, child_unit, edge, kind)
    source: PathRequest | VersionRequest
    if child.source is None:
        source = PathRequest(Path(os.path.relpath(members[child.id], members[parent.id])).as_posix())
    elif declared == CRATES_IO:
        source = VersionRequest(child.version)
    else:
        raise VerificationError(UnsupportedSource(source=str(declared)))
    return alias, DependencyRequest(child.name, tuple(sorted(child_unit.features)), source)


def has_path(value: TomlValue) -> bool:
    match value:
        case dict():
            return isinstance(value.get("path"), str) or any(has_path(item) for item in value.values())
        case list():
            return any(has_path(item) for item in value)
    return False


def check_original_contracts(contracts: tuple[OriginalContract, ...], active_members: set[str],
                             scripted_members: set[str]) -> None:
    for contract in contracts:
        if contract.member not in active_members:
            continue
        if contract.links:
            raise VerificationError(OriginalLinks(member=contract.member))
        if contract.build_script is not None and contract.member not in scripted_members:
            raise VerificationError(OriginalBuildScript(member=contract.member))


@dataclass
class StubRequirements:
    declarations: dict[str, dict[DependencyKind, dict[str, DependencyRequest]]]
    features: dict[str, set[str]]
    build_scripts: set[str] = field(default_factory=set)
    proc_macros: set[str] = field(default_factory=set)

    @classmethod
    def for_packages(cls, identities: dict[str, str]) -> Self:
        return cls({identity: {} for identity in identities}, {identity: set() for identity in identities})

    def dependency_kind_of(self, unit: Unit, identity: str) -> DependencyKind:
        if unit.target.kind == (TargetKind.CUSTOM_BUILD,):
            self.build_scripts.add(identity)
            return DependencyKind.BUILD
        if unit.target.kind == (TargetKind.PROC_MACRO,):
            self.proc_macros.add(identity)
        elif not set(unit.target.kind) <= SUPPORTED_LOCAL_KINDS:
            raise VerificationError(UnsupportedTargetKind(kinds=tuple(kind.value for kind in unit.target.kind)))
        return DependencyKind.NORMAL

    def declare(self, parent: Package, kind: DependencyKind, alias: str, request: DependencyRequest) -> None:
        dependencies = self.declarations[parent.id].setdefault(kind, {})
        if alias in dependencies and dependencies[alias] != request:
            raise VerificationError(ConflictingRequests(package=parent.name, table=kind.table, alias=alias))
        dependencies[alias] = request


def stub_requirements(units: list[Unit], graph: UnitGraph, packages: dict[str, Package],
                      members: dict[str, str]) -> StubRequirements:
    requirements = StubRequirements.for_packages(members)
    for unit in units:
        parent = packages[unit.pkg_id]
        if parent.source is not None:
            if any(packages[graph.units[edge.index].pkg_id].source is None for edge in unit.dependencies):
                raise VerificationError(ExternalDependsOnLocal())
            continue
        if parent.links:
            raise VerificationError(LocalLinks(package=parent.name))
        requirements.features[parent.id].update(unit.features)
        if unit.mode is UnitMode.RUN_CUSTOM_BUILD:
            continue
        if unit.mode is not UnitMode.BUILD:
            raise VerificationError(UnsupportedMode(mode=unit.mode.value))
        kind = requirements.dependency_kind_of(unit, parent.id)
        for edge in unit.dependencies:
            child_unit = graph.units[edge.index]
            child = packages[child_unit.pkg_id]
            if child.id != parent.id:
                alias, request = dependency_request(parent, child, child_unit, edge, kind, members)
                requirements.declare(parent, kind, alias, request)
    return requirements


def canonical_crate(package: Package, member: str, requirements: StubRequirements) -> CanonicalCrate:
    scripted = package.id in requirements.build_scripts
    manifest = StubManifest(
        name=package.name,
        edition=package.edition,
        version=CONTRACT.stub.version,
        library=CONTRACT.stub.library,
        build_script=CONTRACT.stub.build_script if scripted else None,
        proc_macro=package.id in requirements.proc_macros,
        features=tuple(sorted(requirements.features[package.id])),
        dependencies=requirements.declarations[package.id],
    )
    targets = [StubTarget(CONTRACT.stub.library, main=False)]
    if scripted:
        targets.append(StubTarget(CONTRACT.stub.build_script, main=True))
    return CanonicalCrate(member, manifest.to_toml(), tuple(targets))


def workspace_manifest(original: DocumentTable, crates: tuple[CanonicalCrate, ...]) -> TomlTable:
    copied = original.toml_table()
    manifest: TomlTable = {key: copy.deepcopy(copied[key]) for key in COPIED_SECTIONS if key in copied}
    resolver = original.object("workspace").optional_string("resolver")
    manifest["workspace"] = {"resolver": DEFAULT_RESOLVER if resolver is None else resolver,
                             "members": [crate.member for crate in crates]}
    return manifest


def canonical_plan(seed: Seed, graph: UnitGraph, metadata: CargoMetadata, selection: Selection) -> CanonicalPlan:
    packages = {package.id: package for package in metadata.packages}
    units = selected_units(graph)
    if any(unit.pkg_id not in packages for unit in units):
        raise VerificationError(PackageIdentityMismatch())
    members = {unit.pkg_id: member_path(packages[unit.pkg_id], metadata.workspace_root)
               for unit in units if packages[unit.pkg_id].source is None}
    if len(set(members.values())) != len(members):
        raise VerificationError(SharedMember())
    original = seed.workspace_manifest
    copied = original.toml_table()
    if any(has_path(copied[section]) for section in PATCH_SECTIONS if section in copied):
        raise VerificationError(PathPatch())
    requirements = stub_requirements(units, graph, packages, members)
    check_original_contracts(seed.original_contracts, set(members.values()),
                             {members[identity] for identity in requirements.build_scripts})
    if not set(selection.packages) <= {packages[identity].name for identity in members}:
        raise VerificationError(RootPackageMissing())
    crates = tuple(canonical_crate(packages[identity], member, requirements)
                   for identity, member in sorted(members.items(), key=lambda item: item[1]))
    return CanonicalPlan(schema=CONTRACT.schema, selection=selection, manifest=workspace_manifest(original, crates),
                         crates=crates)


def write_stub_targets(member: Path, targets: tuple[StubTarget, ...]) -> None:
    for target in targets:
        source = member / target.path
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(f"{CONTRACT.stub.main_source if target.main else CONTRACT.stub.library_source}\n")


def materialize(plan: CanonicalPlan, destination: Path, lock: CargoLock) -> None:
    destination.mkdir(parents=True)
    (destination / "Cargo.toml").write_text(tomli_w.dumps(plan.manifest))
    (destination / "Cargo.lock").write_text(tomli_w.dumps(lock.to_toml()))
    for crate in plan.crates:
        member = destination / crate.member
        member.mkdir(parents=True)
        (member / "Cargo.toml").write_text(tomli_w.dumps(crate.manifest))
        write_stub_targets(member, crate.targets)


def verify_units(expected: UnitGraph, actual: UnitGraph) -> None:
    wanted, found = set(cargo_plan.external_units(expected)), set(cargo_plan.external_units(actual))
    if wanted != found:
        raise VerificationError(ExternalContextsChanged(missing=len(wanted - found), extra=len(found - wanted)))
