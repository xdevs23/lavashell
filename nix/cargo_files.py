# SPDX-License-Identifier: GPL-3.0-only

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Self

from cargo_models import DependencyKind
from document_tables import DocumentTable, MalformedDocument, TomlTable, TomlValue, ValueKind
from verification_errors import VerificationError


def read_toml(path: Path) -> DocumentTable:
    return DocumentTable.decoded(tomllib.loads(path.read_text()), str(path))


def package_name(directory: Path) -> str:
    return read_toml(directory / "Cargo.toml").object("package").string("name")


# https://doc.rust-lang.org/cargo/reference/pkgid-spec.html
@dataclass(frozen=True)
class LockReference:
    name: str
    version: str | None
    source: str | None

    @classmethod
    def parse(cls, text: str) -> Self:
        name, _, rest = text.partition(" ")
        version, _, source = rest.partition(" ")
        return cls(name, version or None, source.removeprefix("(").removesuffix(")") or None)

    def text(self) -> str:
        parts = [self.name, *([self.version] if self.version else []), *([f"({self.source})"] if self.source else [])]
        return " ".join(parts)


@dataclass(frozen=True)
class LockedPackage:
    name: str
    version: str
    source: str | None
    checksum: str | None
    dependencies: tuple[LockReference, ...]

    @classmethod
    def from_table(cls, raw: DocumentTable) -> Self:
        return cls(
            name=raw.string("name"),
            version=raw.string("version"),
            source=raw.optional_string("source"),
            checksum=raw.optional_string("checksum"),
            dependencies=tuple(LockReference.parse(text) for text in raw.strings("dependencies"))
            if raw.has("dependencies") else (),
        )

    def to_toml(self) -> TomlTable:
        table: TomlTable = {"name": self.name, "version": self.version}
        if self.source is not None:
            table["source"] = self.source
        if self.checksum is not None:
            table["checksum"] = self.checksum
        if self.dependencies:
            table["dependencies"] = [reference.text() for reference in self.dependencies]
        return table


# https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
@dataclass(frozen=True)
class CargoLock:
    version: int
    packages: tuple[LockedPackage, ...]

    @classmethod
    def from_table(cls, raw: DocumentTable) -> Self:
        return cls(version=raw.integer("version"),
                   packages=tuple(LockedPackage.from_table(package) for package in raw.objects("package")))

    @classmethod
    def read(cls, path: Path) -> Self:
        return cls.from_table(read_toml(path))

    def to_toml(self) -> TomlTable:
        return {"version": self.version, "package": [package.to_toml() for package in self.packages]}


@dataclass(frozen=True)
class FeatureActivation:
    requirement: str
    features: tuple[str, ...]

    @classmethod
    def exact(cls, version: str, features: tuple[str, ...]) -> Self:
        return cls(requirement="=" + version, features=features)

    @classmethod
    def from_table(cls, raw: DocumentTable) -> Self:
        return cls(requirement=raw.string("version"), features=raw.strings("features"))

    def to_toml(self) -> TomlTable:
        return {"version": self.requirement, "default-features": False, "features": list(self.features)}


type FeatureActivations = dict[DependencyKind, dict[str, FeatureActivation]]


def activations_from_table(raw: DocumentTable) -> FeatureActivations:
    activations: FeatureActivations = {}
    for table in raw.keys():
        try:
            kind = DependencyKind.of_table(table)
        except ValueError:
            raise VerificationError(MalformedDocument(location=f"{raw.location}.{table}",
                                                      expected=ValueKind.DEPENDENCY_TABLE)) from None
        packages = raw.object(table)
        activations[kind] = {name: FeatureActivation.from_table(packages.object(name)) for name in packages.keys()}
    return activations


def activations_to_toml(activations: FeatureActivations) -> TomlTable:
    tables: TomlTable = {}
    for kind, packages in activations.items():
        dependencies: dict[str, TomlValue] = {name: activation.to_toml() for name, activation in packages.items()}
        tables[kind.table] = dependencies
    return tables


@dataclass(frozen=True)
class PathRequest:
    path: str


@dataclass(frozen=True)
class VersionRequest:
    version: str


@dataclass(frozen=True)
class DependencyRequest:
    package: str
    features: tuple[str, ...]
    source: PathRequest | VersionRequest

    def to_toml(self) -> TomlTable:
        table: TomlTable = {"package": self.package, "default-features": False, "features": list(self.features)}
        match self.source:
            case PathRequest(path):
                table["path"] = path
            case VersionRequest(version):
                table["version"] = "=" + version
        return table


@dataclass(frozen=True)
class StubManifest:
    name: str
    edition: str
    version: str
    library: str
    build_script: str | None
    proc_macro: bool
    features: tuple[str, ...]
    dependencies: dict[DependencyKind, dict[str, DependencyRequest]]

    def to_toml(self) -> TomlTable:
        library: TomlTable = {"path": self.library}
        if self.proc_macro:
            library["proc-macro"] = True
        manifest: TomlTable = {
            "package": {"name": self.name, "version": self.version, "edition": self.edition,
                        "autobins": False, "autotests": False, "autoexamples": False, "autobenches": False,
                        "build": False if self.build_script is None else self.build_script},
            "lib": library,
            "features": {feature: [] for feature in self.features},
        }
        for kind, requests in self.dependencies.items():
            dependencies: dict[str, TomlValue] = {alias: request.to_toml() for alias, request in requests.items()}
            manifest[kind.table] = dependencies
        return manifest
