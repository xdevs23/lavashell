# SPDX-License-Identifier: GPL-3.0-only

import copy
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Self

from cargo_files import CargoLock, FeatureActivations, activations_from_table, activations_to_toml
from dependency_workspace import CanonicalPlan
from document_tables import DocumentTable, TomlTable, TomlValue
from projection_contract import Layer, layer_named


@dataclass(frozen=True)
class LayerPlan:
    seed_hash: str
    plan: CanonicalPlan

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(seed_hash=raw.string("seedHash"), plan=CanonicalPlan.from_json(raw.object("plan")))

    def to_json(self) -> TomlTable:
        return {"seedHash": self.seed_hash, "plan": self.plan.to_json()}


type DependencyPlans = dict[str, dict[Layer, LayerPlan]]


def plans_from_json(raw: DocumentTable) -> DependencyPlans:
    plans: DependencyPlans = {}
    for target in raw.keys():
        layers = raw.object(target)
        plans[target] = {layer_named(layer, layers.location): LayerPlan.from_json(layers.object(layer))
                         for layer in layers.keys()}
    return plans


def plans_to_json(plans: DependencyPlans) -> TomlTable:
    tables: TomlTable = {}
    for target, layers in plans.items():
        plans_of_target: dict[str, TomlValue] = {layer.value: plan.to_json() for layer, plan in layers.items()}
        tables[target] = plans_of_target
    return tables


def retained_plans(plans: DependencyPlans, obsolete: Layer, target: str) -> DependencyPlans:
    retained = {}
    for platform, layers in plans.items():
        if active := {layer: plan for layer, plan in layers.items() if layer is not obsolete and platform != target}:
            retained[platform] = active
    return retained


def stored_plans(raw: DocumentTable) -> DependencyPlans:
    return plans_from_json(raw.object("dependencyPlans")) if raw.has("dependencyPlans") else {}


def write_json(path: Path, raw: TomlTable) -> None:
    path.write_text(json.dumps(raw, indent=2, sort_keys=True) + "\n")


@dataclass
class ProjectionFile:
    normal_lock: CargoLock
    dependency_plans: DependencyPlans = field(default_factory=dict)
    engine_lock: CargoLock | None = None
    features: FeatureActivations | None = None
    external_units: list[str] | None = None

    @classmethod
    def read(cls, path: Path) -> Self:
        raw = DocumentTable.parse(path.read_text(), path.name)
        engine_lock = raw.optional_object("engineLock")
        features = raw.optional_object("features")
        return cls(
            normal_lock=CargoLock.from_table(raw.object("normalLock")),
            dependency_plans=stored_plans(raw),
            engine_lock=None if engine_lock is None else CargoLock.from_table(engine_lock),
            features=None if features is None else activations_from_table(features),
            external_units=list(raw.strings("externalUnits")) if raw.has("externalUnits") else None,
        )

    def write(self, path: Path) -> None:
        raw: TomlTable = {"normalLock": self.normal_lock.to_toml(),
                          "dependencyPlans": plans_to_json(self.dependency_plans)}
        if self.engine_lock is not None:
            raw["engineLock"] = self.engine_lock.to_toml()
        if self.features is not None:
            raw["features"] = activations_to_toml(self.features)
        if self.external_units is not None:
            external_units: list[TomlValue] = list(self.external_units)
            raw["externalUnits"] = external_units
        write_json(path, raw)

    def prepared(self, layer: Layer, target: str) -> "ProjectionFile":
        obsolete = Layer.REGISTRY if layer is Layer.ENGINE else Layer.ENGINE
        prepared = copy.deepcopy(self)
        prepared.dependency_plans = retained_plans(prepared.dependency_plans, obsolete, target)
        if layer is Layer.REGISTRY:
            prepared.engine_lock = None
            prepared.features = None
            prepared.external_units = None
        return prepared


@dataclass
class NativeProjection:
    lock: CargoLock
    dependency_plans: DependencyPlans = field(default_factory=dict)

    @classmethod
    def read(cls, path: Path) -> Self:
        raw = DocumentTable.parse(path.read_text(), path.name)
        return cls(lock=CargoLock.from_table(raw.object("lock")), dependency_plans=stored_plans(raw))

    def write(self, path: Path) -> None:
        raw: TomlTable = {"lock": self.lock.to_toml()}
        if self.dependency_plans:
            raw["dependencyPlans"] = plans_to_json(self.dependency_plans)
        write_json(path, raw)

    def prepared(self, lock: CargoLock, target: str) -> "NativeProjection":
        return NativeProjection(lock, retained_plans(self.dependency_plans, Layer.ENGINE, target))
