# SPDX-License-Identifier: GPL-3.0-only

import json
from dataclasses import asdict, dataclass
from enum import Enum
from pathlib import Path, PurePosixPath
from typing import Self
from urllib.parse import urlsplit

from document_tables import DocumentTable, DocumentValue


class UnitMode(Enum):
    BUILD = "build"
    CHECK = "check"
    TEST = "test"
    DOC = "doc"
    DOC_TEST = "doctest"
    DOC_SCRAPE = "docscrape"
    RUN_CUSTOM_BUILD = "run-custom-build"


class CrateType(Enum):
    BIN = "bin"
    LIB = "lib"
    RLIB = "rlib"
    DYLIB = "dylib"
    CDYLIB = "cdylib"
    STATICLIB = "staticlib"
    PROC_MACRO = "proc-macro"


class TargetKind(Enum):
    BIN = "bin"
    LIB = "lib"
    RLIB = "rlib"
    DYLIB = "dylib"
    CDYLIB = "cdylib"
    STATICLIB = "staticlib"
    PROC_MACRO = "proc-macro"
    CUSTOM_BUILD = "custom-build"
    TEST = "test"
    BENCH = "bench"
    EXAMPLE = "example"


class MessageReason(Enum):
    COMPILER_ARTIFACT = "compiler-artifact"
    BUILD_FINISHED = "build-finished"


class DependencyKind(Enum):
    NORMAL = "normal"
    BUILD = "build"
    DEV = "dev"

    @property
    def table(self) -> str:
        match self:
            case DependencyKind.NORMAL:
                return "dependencies"
            case DependencyKind.BUILD:
                return "build-dependencies"
            case DependencyKind.DEV:
                return "dev-dependencies"

    @classmethod
    def of_table(cls, table: str) -> "DependencyKind":
        for kind in cls:
            if kind.table == table:
                return kind
        raise ValueError(table)


def enum_value(member: Enum) -> object:
    return member.value


def canonical_json(model: "UnitIdentity | UnitRecord") -> str:
    return json.dumps(asdict(model), sort_keys=True, default=enum_value)


# https://doc.rust-lang.org/cargo/reference/pkgid-spec.html
@dataclass(frozen=True)
class PackageId:
    name: str
    version: str

    @classmethod
    def parse(cls, text: str) -> Self:
        source, _, fragment = text.partition("#")
        name, separator, version = fragment.rpartition("@")
        return cls(name if separator else PurePosixPath(urlsplit(source).path).name, version)


# https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph
@dataclass(frozen=True)
class Profile:
    name: str
    opt_level: str
    lto: str
    codegen_backend: str | None
    codegen_units: int | None
    debuginfo: str | int | None
    debug_assertions: bool
    overflow_checks: bool
    rpath: bool
    incremental: bool
    panic: str
    split_debuginfo: str | None
    strip: DocumentValue

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            name=raw.string("name"),
            opt_level=raw.string("opt_level"),
            lto=raw.string("lto"),
            codegen_backend=raw.optional_string("codegen_backend"),
            codegen_units=None if raw.value("codegen_units") is None else raw.integer("codegen_units"),
            debuginfo=raw.scalar("debuginfo"),
            debug_assertions=raw.boolean("debug_assertions"),
            overflow_checks=raw.boolean("overflow_checks"),
            rpath=raw.boolean("rpath"),
            incremental=raw.boolean("incremental"),
            panic=raw.string("panic"),
            split_debuginfo=raw.optional_string("split_debuginfo"),
            strip=raw.value("strip"),
        )


@dataclass(frozen=True)
class TargetIdentity:
    name: str
    kind: tuple[TargetKind, ...]
    crate_types: tuple[CrateType, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            name=raw.string("name"),
            kind=tuple(TargetKind(kind) for kind in raw.strings("kind")),
            crate_types=tuple(CrateType(crate_type) for crate_type in raw.strings("crate_types")),
        )


@dataclass(frozen=True)
class UnitIdentity:
    package: str
    target: TargetIdentity
    profile: Profile
    platform: str | None
    mode: UnitMode
    features: tuple[str, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            package=raw.string("package"),
            target=TargetIdentity.from_json(raw.object("target")),
            profile=Profile.from_json(raw.object("profile")),
            platform=raw.optional_string("platform"),
            mode=UnitMode(raw.string("mode")),
            features=raw.strings("features"),
        )

    @classmethod
    def parse(cls, text: str) -> Self:
        return cls.from_json(DocumentTable.parse(text, "unit"))

    def text(self) -> str:
        return canonical_json(self)


@dataclass(frozen=True)
class UnitRecord:
    unit: UnitIdentity
    dependencies: tuple[str, ...]

    @classmethod
    def parse(cls, text: str) -> Self:
        raw = DocumentTable.parse(text, "unit record")
        return cls(unit=UnitIdentity.from_json(raw.object("unit")), dependencies=raw.strings("dependencies"))

    def text(self) -> str:
        return canonical_json(self)


@dataclass(frozen=True)
class Edge:
    index: int
    extern_crate_name: str
    noprelude: bool
    nounused: bool
    public: bool

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            index=raw.integer("index"),
            extern_crate_name=raw.string("extern_crate_name"),
            noprelude=raw.flag("noprelude"),
            nounused=raw.flag("nounused"),
            public=raw.flag("public"),
        )

    @property
    def has_flags(self) -> bool:
        return self.noprelude or self.nounused or self.public


@dataclass(frozen=True)
class Unit:
    pkg_id: str
    target: TargetIdentity
    profile: Profile
    platform: str | None
    mode: UnitMode
    features: tuple[str, ...]
    dependencies: tuple[Edge, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            pkg_id=raw.string("pkg_id"),
            target=TargetIdentity.from_json(raw.object("target")),
            profile=Profile.from_json(raw.object("profile")),
            platform=raw.optional_string("platform"),
            mode=UnitMode(raw.string("mode")),
            features=raw.strings("features"),
            dependencies=tuple(Edge.from_json(edge) for edge in raw.objects("dependencies")),
        )

    @property
    def identity(self) -> UnitIdentity:
        return UnitIdentity(self.pkg_id, self.target, self.profile, self.platform, self.mode, self.features)


@dataclass(frozen=True)
class UnitGraph:
    units: tuple[Unit, ...]
    roots: tuple[int, ...]

    @classmethod
    def parse(cls, text: str) -> Self:
        raw = DocumentTable.parse(text, "unit graph")
        return cls(units=tuple(Unit.from_json(unit) for unit in raw.objects("units")), roots=raw.integers("roots"))


@dataclass(frozen=True)
class PackageDependency:
    name: str
    rename: str | None
    kind: DependencyKind
    source: str | None

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        kind = raw.optional_string("kind")
        return cls(
            name=raw.string("name"),
            rename=raw.optional_string("rename"),
            kind=DependencyKind.NORMAL if kind is None else DependencyKind(kind),
            source=raw.optional_string("source"),
        )


@dataclass(frozen=True)
class Package:
    id: str
    name: str
    version: str
    edition: str
    source: str | None
    manifest_path: Path
    dependencies: tuple[PackageDependency, ...]
    links: str | None

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            id=raw.string("id"),
            name=raw.string("name"),
            version=raw.string("version"),
            edition=raw.string("edition"),
            source=raw.optional_string("source"),
            manifest_path=Path(raw.string("manifest_path")),
            dependencies=tuple(PackageDependency.from_json(dependency) for dependency in raw.objects("dependencies")),
            links=raw.optional_string("links"),
        )


@dataclass(frozen=True)
class CargoMetadata:
    workspace_root: Path
    packages: tuple[Package, ...]

    @classmethod
    def parse(cls, text: str) -> Self:
        raw = DocumentTable.parse(text, "cargo metadata")
        return cls(
            workspace_root=Path(raw.string("workspace_root")),
            packages=tuple(Package.from_json(package) for package in raw.objects("packages")),
        )


# https://doc.rust-lang.org/cargo/reference/external-tools.html#compiler-messages
@dataclass(frozen=True)
class ArtifactProfile:
    opt_level: str
    debuginfo: str | int | None
    debug_assertions: bool
    overflow_checks: bool
    test: bool

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            opt_level=raw.string("opt_level"),
            debuginfo=raw.scalar("debuginfo"),
            debug_assertions=raw.boolean("debug_assertions"),
            overflow_checks=raw.boolean("overflow_checks"),
            test=raw.boolean("test"),
        )


@dataclass(frozen=True)
class ArtifactTarget:
    name: str
    kind: tuple[TargetKind, ...]

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(name=raw.string("name"), kind=tuple(TargetKind(kind) for kind in raw.strings("kind")))


@dataclass(frozen=True)
class CompilerArtifact:
    package_id: str
    target: ArtifactTarget
    profile: ArtifactProfile
    features: tuple[str, ...]
    filenames: tuple[Path, ...]
    fresh: bool

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            package_id=raw.string("package_id"),
            target=ArtifactTarget.from_json(raw.object("target")),
            profile=ArtifactProfile.from_json(raw.object("profile")),
            features=raw.strings("features"),
            filenames=tuple(Path(name) for name in raw.strings("filenames")),
            fresh=raw.boolean("fresh"),
        )

    @property
    def identity(self) -> "ArtifactIdentity":
        return ArtifactIdentity(self.package_id, self.target, self.profile, self.features,
                                tuple(sorted(path.name for path in self.filenames)))

    @property
    def package_name(self) -> str:
        return PackageId.parse(self.package_id).name


@dataclass(frozen=True)
class ArtifactIdentity:
    package_id: str
    target: ArtifactTarget
    profile: ArtifactProfile
    features: tuple[str, ...]
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class BuildFinished:
    success: bool

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(success=raw.boolean("success"))


@dataclass(frozen=True)
class OtherOutput:
    pass


type CargoMessage = CompilerArtifact | BuildFinished | OtherOutput


def read_message(line: str) -> CargoMessage:
    try:
        raw = DocumentTable.parse(line, "cargo message")
    except json.JSONDecodeError:
        return OtherOutput()
    try:
        reason = MessageReason(raw.string("reason"))
    except ValueError:
        return OtherOutput()
    match reason:
        case MessageReason.COMPILER_ARTIFACT:
            return CompilerArtifact.from_json(raw)
        case MessageReason.BUILD_FINISHED:
            return BuildFinished.from_json(raw)
