# SPDX-License-Identifier: GPL-3.0-only

import copy
import json
import os
import tomllib
from pathlib import Path

SCHEMA = 1
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def selected_units(graph):
    pending = list(graph["roots"])
    seen = set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        if not 0 <= index < len(graph["units"]):
            raise ValueError(f"invalid Cargo unit index {index}")
        seen.add(index)
        pending.extend(edge["index"] for edge in graph["units"][index]["dependencies"])
    if not seen:
        raise ValueError("Cargo selected no units")
    return [graph["units"][index] for index in sorted(seen)]


def _member(package, workspace_root):
    try:
        member = Path(package["manifest_path"]).parent.relative_to(workspace_root).as_posix()
    except ValueError as error:
        raise ValueError(f"external path package cannot be replaced by a workspace stub: {package['id']}") from error
    if member in ("", ".") or ".." in Path(member).parts:
        raise ValueError("dependency projections require a virtual workspace and contained member paths")
    return member


def _declaration(parent, child, unit, edge, kind):
    candidates = set()
    for dependency in parent["dependencies"]:
        if dependency["name"] != child["name"] or (dependency["kind"] or "normal") != kind:
            continue
        alias = dependency.get("rename") or dependency["name"]
        expected = (dependency.get("rename") or unit["target"]["name"]).replace("-", "_")
        if expected == edge["extern_crate_name"]:
            candidates.add((alias, dependency["source"]))
    if len(candidates) != 1:
        raise ValueError(f"ambiguous/missing dependency alias in {parent['name']}: {edge['extern_crate_name']}")
    return candidates.pop()


def _request(parent, child, child_unit, edge, kind, members):
    if any(edge.get(flag, False) for flag in ("noprelude", "nounused", "public")):
        raise ValueError(f"unsupported dependency-edge flags: {parent['name']} -> {child['name']}")
    alias, declared = _declaration(parent, child, child_unit, edge, kind)
    request = {"package": child["name"], "default-features": False, "features": sorted(child_unit["features"])}
    if child["source"] is None:
        request["path"] = Path(os.path.relpath(members[child["id"]], members[parent["id"]])).as_posix()
    elif declared == CRATES_IO:
        # The plan contains the patches of the workspace, so a patched crate resolves to the patch again.
        request["version"] = "=" + child["version"]
    else:
        raise ValueError(f"unsupported declared source (registry and git configuration are not inferred): {declared}")
    return alias, request


def _has_path(value):
    if isinstance(value, dict):
        return isinstance(value.get("path"), str) or any(_has_path(item) for item in value.values())
    return isinstance(value, list) and any(_has_path(item) for item in value)


def _check_original_contracts(contracts, active_members, scripted_members):
    for contract in contracts:
        if contract["member"] not in active_members:
            continue
        if contract["links"]:
            raise ValueError(f"selected engine member has an unsupported original links contract: {contract['member']}")
        if contract["build"] and contract["member"] not in scripted_members:
            raise ValueError(f"selected engine member has an original build script that its seed did not follow: "
                             f"{contract['member']}")


def canonical_plan(seed, graph, metadata, selection):
    """Return a workspace in which each local crate of the selected unit graph is an empty stub with the same
    dependencies and features. The plan contains the workspace manifest, the stub crates and the selection, but no lock
    file. Cargo resolves and checks the lock file of this workspace afterwards."""
    workspace_root = Path(metadata["workspace_root"])
    packages = {package["id"]: package for package in metadata["packages"]}
    units = selected_units(graph)
    if any(unit["pkg_id"] not in packages for unit in units):
        raise ValueError("Cargo metadata and unit graph disagree on package identities")
    locals_ = {unit["pkg_id"] for unit in units if packages[unit["pkg_id"]]["source"] is None}
    members = {identity: _member(packages[identity], workspace_root) for identity in locals_}
    if len(set(members.values())) != len(members):
        raise ValueError("multiple resolved packages occupy one workspace member")
    original = seed["workspace_manifest"]
    for kind in ("patch", "replace"):
        if _has_path(original.get(kind, {})):
            raise ValueError("path patches/replacements need real source handling, not dummy dependency stubs")
    declarations = {identity: {} for identity in locals_}
    features = {identity: set() for identity in locals_}
    build_scripts = set()
    proc_macros = set()
    for unit in units:
        parent = packages[unit["pkg_id"]]
        if parent["source"] is not None:
            if any(packages[graph["units"][edge["index"]]["pkg_id"]]["source"] is None
                   for edge in unit["dependencies"]):
                raise ValueError("external units depend on local source; a dummy layer cannot preserve that input")
            continue
        identity = parent["id"]
        if parent.get("links"):
            raise ValueError(f"local links/build output cannot be replaced by an empty stub: {parent['name']}")
        features[identity].update(unit["features"])
        if unit["mode"] == "run-custom-build":
            continue
        if unit["mode"] != "build":
            raise ValueError(f"unsupported dependency-layer Cargo mode: {unit['mode']}")
        target_kind = unit["target"]["kind"]
        if target_kind == ["custom-build"]:
            build_scripts.add(identity)
            kind, table = "build", "build-dependencies"
        else:
            if target_kind == ["proc-macro"]:
                proc_macros.add(identity)
            elif not set(target_kind) <= {"lib", "bin", "rlib", "cdylib", "staticlib"}:
                raise ValueError(f"unsupported local target kind: {target_kind}")
            kind, table = "normal", "dependencies"
        for edge in unit["dependencies"]:
            child_unit = graph["units"][edge["index"]]
            child = packages[child_unit["pkg_id"]]
            if child["id"] == identity:
                # Cargo itself adds the edges to the library and the build script of the same package. The manifest
                # does not declare them.
                continue
            alias, request = _request(parent, child, child_unit, edge, kind, members)
            dependencies = declarations[identity].setdefault(table, {})
            if alias in dependencies and dependencies[alias] != request:
                raise ValueError(
                    f"distinct host/target requests share {parent['name']}.{table}.{alias}; "
                    "cannot merge feature contexts into one manifest"
                )
            dependencies[alias] = request
    _check_original_contracts(seed.get("originalContracts", []), set(members.values()),
                              {members[identity] for identity in build_scripts})
    crates = []
    for identity, member in sorted(members.items(), key=lambda item: item[1]):
        package = packages[identity]
        manifest = {
            "package": {"name": package["name"], "version": "0.0.0", "edition": package["edition"],
                        "autobins": False, "autotests": False, "autoexamples": False, "autobenches": False,
                        "build": "build.rs" if identity in build_scripts else False},
            "lib": {"path": "src/lib.rs"},
            "features": {feature: [] for feature in sorted(features[identity])},
            **declarations[identity],
        }
        if identity in proc_macros:
            manifest["lib"]["proc-macro"] = True
        targets = [{"path": "src/lib.rs", "main": False}]
        if identity in build_scripts:
            targets.append({"path": "build.rs", "main": True})
        crates.append({"member": member, "manifest": manifest, "targets": targets})
    names = {crate["manifest"]["package"]["name"] for crate in crates}
    if not set(selection["packages"]) <= names:
        raise ValueError("selected root package missing from active local closure")
    manifest = {key: copy.deepcopy(original[key]) for key in ("profile", "patch", "replace") if key in original}
    manifest["workspace"] = {"resolver": original["workspace"].get("resolver", "1"),
                             "members": [crate["member"] for crate in crates]}
    return {"schema": SCHEMA, "selection": copy.deepcopy(selection), "manifest": manifest, "crates": crates}


def _string(value):
    # A JSON string is a valid TOML basic string, with two exceptions. TOML forbids a raw DEL character, so DEL is
    # escaped. TOML strings contain only Unicode scalar values, so a surrogate code point is rejected.
    if any(0xD800 <= ord(character) <= 0xDFFF for character in value):
        raise ValueError("TOML strings cannot contain Unicode surrogate code points")
    return json.dumps(value, ensure_ascii=False).replace("\x7f", "\\u007f")


def _value(value):
    if isinstance(value, dict):
        return "{ " + ", ".join(_string(key) + " = " + _value(item)
                                 for key, item in sorted(value.items())) + " }"
    if isinstance(value, list):
        return "[ " + ", ".join(_value(item) for item in value) + " ]"
    if isinstance(value, str):
        return _string(value)
    if isinstance(value, (bool, int)):
        return json.dumps(value)
    raise ValueError(f"unsupported Cargo TOML value type: {type(value).__name__}")


def manifest_text(manifest):
    sections = []
    for key, value in sorted(manifest.items()):
        for entry in value if isinstance(value, list) else [value]:
            heading = f"[[{_string(key)}]]" if isinstance(value, list) else f"[{_string(key)}]"
            sections.append(heading + "\n" + "".join(_string(name) + " = " + _value(item) + "\n"
                                                      for name, item in sorted(entry.items())) + "\n")
    rendered = "".join(sections)
    if tomllib.loads(rendered) != manifest:
        raise ValueError("canonical TOML serialization changed manifest semantics")
    return rendered


def lock_text(lock):
    return f"version = {lock['version']}\n\n" + manifest_text({"package": lock["package"]})


def materialize(plan, destination, lock):
    destination.mkdir(parents=True)
    (destination / "Cargo.toml").write_text(manifest_text(plan["manifest"]))
    (destination / "Cargo.lock").write_text(lock_text(lock))
    for crate in plan["crates"]:
        member = destination / crate["member"]
        member.mkdir(parents=True)
        (member / "Cargo.toml").write_text(manifest_text(crate["manifest"]))
        for target in crate["targets"]:
            source = member / target["path"]
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text("fn main() {}\n" if target["main"] else "\n")


def verify_units(expected, actual, external_units):
    wanted, found = set(external_units(expected)), set(external_units(actual))
    if wanted != found:
        raise ValueError(f"canonical dependency plan changed external contexts: missing={len(wanted - found)}, "
                         f"extra={len(found - wanted)}; do not publish this projection")
