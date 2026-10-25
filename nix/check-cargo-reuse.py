# SPDX-License-Identifier: GPL-3.0-only
"""Read the artifact messages recorded from Cargo builds and check that a later build reused the artifacts of an
earlier one.

verify() requires every artifact of a package whose name starts with a prefix to be identical in both builds and
marked fresh in the later build, which means that Cargo reused it.
"""

import json
from pathlib import Path


def records(path):
    messages = []
    for line in path.read_text().splitlines():
        if line.startswith("{"):
            messages.append(json.loads(line))
    assert any(m.get("reason") == "build-finished" and m["success"] for m in messages), (
        f"missing successful Cargo build result: {path}"
    )
    return [m for m in messages if m.get("reason") == "compiler-artifact"]


def identity(message):
    return json.dumps(
        {
            "package": message["package_id"],
            "target": [message["target"]["name"], message["target"]["kind"]],
            "profile": message["profile"],
            "features": message["features"],
            "outputs": sorted(Path(path).name for path in message["filenames"]),
        },
        sort_keys=True,
    )


def package_name(message):
    identifier = message["package_id"].rsplit("#", 1)[-1]
    if "@" in identifier:
        return identifier.split("@", 1)[0]
    return message["target"]["name"].replace("_", "-")


def verify(reference, current, prefix, exclude=""):
    expected = {identity(m): m for m in reference if package_name(m).startswith(prefix) and package_name(m) != exclude}
    actual = {identity(m): m for m in current if package_name(m).startswith(prefix) and package_name(m) != exclude}
    assert expected, f"no reference compiler artifacts match {prefix}"
    assert actual.keys() == expected.keys(), "dependency features, profiles or artifact identities changed"
    assert all(m["fresh"] for m in actual.values()), "Cargo rebuilt a dependency expected to be reused"
    return len(actual)

