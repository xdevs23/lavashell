# SPDX-License-Identifier: GPL-3.0-only

import json

EXTERNAL_SOURCES = ("registry+", "git+")


def unit_key(unit):
    return json.dumps({
        "package": unit["pkg_id"],
        "target": {key: unit["target"][key] for key in ("name", "kind", "crate_types")},
        "profile": unit["profile"],
        "platform": unit["platform"],
        "mode": unit["mode"],
        "features": unit["features"],
    }, sort_keys=True)


def external_units(graph):
    units = graph["units"]
    return sorted({json.dumps({
        "unit": json.loads(unit_key(unit)),
        "dependencies": sorted(unit_key(units[dependency["index"]]) for dependency in unit["dependencies"]),
    }, sort_keys=True) for unit in units if unit["pkg_id"].startswith(EXTERNAL_SOURCES)})


def bevy_units(graph):
    units = graph["units"]
    pending = [index for index, unit in enumerate(units)
               if unit["pkg_id"].startswith(EXTERNAL_SOURCES) and unit["target"]["name"] == "bevy"]
    seen = set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        seen.add(index)
        pending.extend(dependency["index"] for dependency in units[index]["dependencies"])
    return {unit_key(units[index]) for index in seen}


def verify(engine, consumer):
    actual = set(external_units(consumer))
    if missing := set(engine) - actual:
        raise ValueError(f"{len(missing)} engine units changed; run nix/refresh-cargo-projections.py")
    available = {json.dumps(json.loads(record)["unit"], sort_keys=True) for record in engine}
    if missing := bevy_units(consumer) - available:
        raise ValueError(f"{len(missing)} Bevy units are uncached; run nix/refresh-cargo-projections.py")
