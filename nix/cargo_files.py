# SPDX-License-Identifier: GPL-3.0-only

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Self

from document_tables import DocumentTable, TomlTable


def read_toml(path: Path) -> DocumentTable:
    return DocumentTable.decoded(tomllib.loads(path.read_text()), str(path))


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
