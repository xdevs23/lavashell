#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import argparse
from dataclasses import dataclass
from pathlib import Path
from typing import Self

import cargo_plan
from cargo_models import UnitGraph
from document_tables import DocumentTable


@dataclass(frozen=True)
class EngineProjection:
    external_units: tuple[str, ...]

    @classmethod
    def read(cls, path: Path) -> Self:
        return cls(external_units=DocumentTable.parse(path.read_text(), path.name).strings("externalUnits"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True, type=Path)
    parser.add_argument("--consumer", required=True, type=Path)
    args = parser.parse_args()
    engine_path: Path = args.engine
    consumer_path: Path = args.consumer
    engine = EngineProjection.read(engine_path)
    cargo_plan.verify(engine.external_units, UnitGraph.parse(consumer_path.read_text()))
    print(f"Verified {len(engine.external_units)} compatible engine external units")


if __name__ == "__main__":
    main()
