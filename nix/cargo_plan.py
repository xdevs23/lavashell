# SPDX-License-Identifier: GPL-3.0-only

from cargo_models import Unit, UnitGraph, UnitRecord
from projection_contract import CONTRACT
from verification_errors import EngineUnitsChanged, EngineUnitsUncached, VerificationError

EXTERNAL_SOURCES = ("registry+", "git+")


def is_external(unit: Unit) -> bool:
    return unit.pkg_id.startswith(EXTERNAL_SOURCES)


def external_units(graph: UnitGraph) -> list[str]:
    units = graph.units
    return sorted({
        UnitRecord(unit.identity, tuple(sorted(units[edge.index].identity.text() for edge in unit.dependencies))).text()
        for unit in units if is_external(unit)
    })


def engine_units(graph: UnitGraph) -> set[str]:
    units = graph.units
    pending = [index for index, unit in enumerate(units)
               if is_external(unit) and unit.target.name == CONTRACT.engine_root]
    seen: set[int] = set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        seen.add(index)
        pending.extend(edge.index for edge in units[index].dependencies)
    return {units[index].identity.text() for index in seen}


def verify(engine: tuple[str, ...] | list[str], consumer: UnitGraph) -> None:
    actual = set(external_units(consumer))
    if missing := set(engine) - actual:
        raise VerificationError(EngineUnitsChanged(count=len(missing)))
    available = {UnitRecord.parse(record).unit.text() for record in engine}
    if missing := engine_units(consumer) - available:
        raise VerificationError(EngineUnitsUncached(count=len(missing)))
