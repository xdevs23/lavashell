# SPDX-License-Identifier: GPL-3.0-only

from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Self

from document_tables import DocumentTable
from verification_errors import Cause, NextStep, VerificationError

CONTRACT_FILE = Path(__file__).with_name("projection-contract.json")


class Layer(Enum):
    ENGINE = "engine"
    REGISTRY = "registry"


@dataclass(frozen=True)
class UnknownLayer(Cause):
    name: str
    location: str
    next_step = NextStep.REFRESH

    def message(self) -> str:
        return f"The layer {self.name} in {self.location} is not a layer of the dependency projections."


def layer_named(name: str, location: str) -> Layer:
    try:
        return Layer(name)
    except ValueError:
        raise VerificationError(UnknownLayer(name=name, location=location)) from None


@dataclass(frozen=True)
class StubLayout:
    version: str
    library: str
    build_script: str
    main_source: str
    library_source: str

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            version=raw.string("version"),
            library=raw.string("library"),
            build_script=raw.string("buildScript"),
            main_source=raw.string("mainSource"),
            library_source=raw.string("librarySource"),
        )


@dataclass(frozen=True)
class ProjectionContract:
    schema: int
    profile: str
    target: str
    projection_directory: str
    engine_root: str
    native_directory: str
    layers: tuple[Layer, ...]
    companions: tuple[str, ...]
    packaged_crates: dict[str, str]
    stub: StubLayout

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            schema=raw.integer("schema"),
            profile=raw.string("profile"),
            target=raw.string("target"),
            projection_directory=raw.string("projectionDirectory"),
            engine_root=raw.string("engineRoot"),
            native_directory=raw.string("nativeDirectory"),
            layers=tuple(layer_named(layer, f"{raw.location}.layers") for layer in raw.strings("layers")),
            companions=raw.strings("companions"),
            packaged_crates=raw.string_map("packagedCrates"),
            stub=StubLayout.from_json(raw.object("stub")),
        )


CONTRACT = ProjectionContract.from_json(DocumentTable.parse(CONTRACT_FILE.read_text(), CONTRACT_FILE.name))
