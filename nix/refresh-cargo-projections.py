#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Refresh the Cargo lock files and dependency plans in nix/cargo-projections without compiling anything.

For each example that builds on the shared engine layer, the script also adds the dependency features that the engine
workspace must enable, until the external build units of the engine match those of the example.
"""

import argparse
import copy
import importlib.util
import itertools
import json
import os
import subprocess
import tempfile
import tomllib
from pathlib import Path

from flake_evaluation import locked_nixpkgs

spec = importlib.util.spec_from_file_location("cargo_plan", Path(__file__).with_name("cargo-plan.py"))
plan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plan)
dependency_spec = importlib.util.spec_from_file_location(
    "dependency_workspace", Path(__file__).with_name("dependency-workspace.py")
)
dependency = importlib.util.module_from_spec(dependency_spec)
dependency_spec.loader.exec_module(dependency)


PACKAGED_CRATES = ["tools/wayland-input", "tools/source-rules", "crates/lavashell-bevy"]


def evaluate(root, nixpkgs, expression):
    source = root / "nix/source.nix"
    prefix = f"""
      let
        root = builtins.toPath {json.dumps(str(root))};
        pkgs = import (builtins.toPath {json.dumps(nixpkgs)}) {{ system = "x86_64-linux"; }};
        sources = import (builtins.toPath {json.dumps(str(source))}) {{ inherit pkgs root; }};
      in {expression}
    """
    result = subprocess.run(["nix", "eval", "--offline", "--impure", "--json", "--expr", prefix],
                            text=True, capture_output=True)
    if result.returncode:
        raise SystemExit(result.stderr)
    return json.loads(result.stdout)


def write(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def workspace(root, nixpkgs, directory, engine, destination):
    layer = "engine" if engine else "registry"
    expression = f"sources.seedPlan (root + {json.dumps('/' + directory)}) {json.dumps(layer)}"
    data = evaluate(root, nixpkgs, expression)
    return write_workspace(data, destination)


def projected_workspace(root, nixpkgs, directory, layer, destination):
    selector = "engineProject" if layer == "engine" else "registryProject"
    directory_expr = f"(root + {json.dumps('/' + directory)})"
    data = evaluate(root, nixpkgs, f"sources.plan {directory_expr} (sources.{selector} {directory_expr})")
    return write_workspace(data, destination)


def write_workspace(data, destination):
    destination.mkdir(parents=True)
    (destination / "Cargo.toml").write_text(data["manifest"])
    (destination / "Cargo.lock").write_text(data["lock"])
    for crate in data["crates"]:
        member = destination / crate["member"]
        member.mkdir(parents=True)
        (member / "Cargo.toml").write_text(crate["manifest"])
        for target in crate["targets"]:
            path = member / target["path"]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fn main() {}\n" if target["main"] else "\n")
    return data


def graph(args, directory, package, renderer, frozen=False):
    command = [args.cargo, "build", "--unit-graph", "-Z", "unstable-options", "--offline",
               "--profile", "packaged", "--target", getattr(args, "target", "x86_64-unknown-linux-gnu"),
               "--package", package]
    if frozen:
        command.append("--frozen")
    if renderer:
        command += ["--package", "lavashell-renderer"]
    environment = os.environ | {"RUSTC_BOOTSTRAP": "1", "RUSTC": args.rustc,
                                "CARGO_TARGET_DIR": str(directory / "target")}
    result = subprocess.run(command, cwd=directory, env=environment, capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(result.stderr)
    return json.loads(result.stdout), tomllib.loads((directory / "Cargo.lock").read_text())


def dependency_plan(args, root, directory, engine, destination):
    seed_path = destination / "seed"
    seed = workspace(root, args.nixpkgs, directory, engine, seed_path)
    package = tomllib.loads((root / directory / "Cargo.toml").read_text())["package"]["name"]
    renderer = any(tomllib.loads(crate["manifest"])["package"]["name"] == "lavashell-renderer"
                   for crate in seed["crates"])
    expected, seed_lock = graph(args, seed_path, package, renderer, frozen=True)
    result = subprocess.run(
        [args.cargo, "metadata", "--offline", "--frozen", "--format-version", "1", "--filter-platform", args.target],
        cwd=seed_path, env=os.environ | {"RUSTC": args.rustc}, capture_output=True, text=True,
    )
    if result.returncode:
        raise ValueError(result.stderr)
    selection = {"target": args.target, "profile": "packaged",
                 "packages": [package] + (["lavashell-renderer"] if renderer else []),
                 "features": []}
    canonical = dependency.canonical_plan(
        {"workspace_manifest": tomllib.loads(seed["manifest"]),
         "originalContracts": seed.get("originalContracts", [])}, expected, json.loads(result.stdout), selection,
    )
    canonical_path = destination / "canonical"
    dependency.materialize(canonical, canonical_path, seed_lock)
    actual, canonical["lock"] = graph(args, canonical_path, package, renderer)
    dependency.verify_units(expected, actual, plan.external_units)
    # Write the lock file again from the lock data as Nix reads it, and plan the build once more. The check then also
    # covers the lock file written by Nix, beyond the one written by Cargo.
    (canonical_path / "Cargo.lock").write_text(dependency.lock_text(canonical["lock"]))
    frozen, _ = graph(args, canonical_path, package, renderer, frozen=True)
    dependency.verify_units(expected, frozen, plan.external_units)
    canonical["externalUnits"] = plan.external_units(frozen)
    return {"seedHash": seed["seedHash"], "plan": canonical}


def store_dependency_plan(args, metadata, layer, entry):
    metadata.setdefault("dependencyPlans", {}).setdefault(args.target, {})[layer] = entry


def prepare_metadata(metadata, engine, target):
    metadata = copy.deepcopy(metadata)
    obsolete = "registry" if engine else "engine"
    retained = {}
    for platform, layers in metadata.get("dependencyPlans", {}).items():
        if active := {layer: record for layer, record in layers.items() if layer != obsolete and platform != target}:
            retained[platform] = active
    metadata["dependencyPlans"] = retained
    if not engine:
        for field in ("features", "engineLock", "externalUnits"):
            metadata.pop(field, None)
    return metadata


def engine_seed_lock(original, baseline, native_names):
    registry = [package for package in baseline["package"] if "source" in package]
    local = [package | {"dependencies": [reference for reference in package.get("dependencies", [])
                                         if reference.split()[0] not in native_names]}
             for package in original["package"] if "source" not in package]
    return baseline | {"package": registry + local}


def activations(engine, consumer):
    def identity(unit):
        data = json.loads(plan.unit_key(unit))
        del data["features"]
        return json.dumps(data, sort_keys=True)
    candidates = {}
    for unit in consumer["units"]:
        candidates.setdefault(identity(unit), []).append(unit)
    result = {}
    for unit in engine["units"]:
        if not unit["pkg_id"].startswith(plan.EXTERNAL_SOURCES) or unit["target"]["kind"] != ["lib"]:
            continue
        matches = candidates.get(identity(unit), [])
        if any(match["features"] == unit["features"] for match in matches):
            continue
        if len(matches) != 1:
            raise ValueError(f"ambiguous feature context: {unit['pkg_id']}")
        name, version = unit["pkg_id"].split("#", 1)[1].rsplit("@", 1)
        kind = "build-dependencies" if unit["platform"] is None else "dependencies"
        result.setdefault(kind, {})[name] = {
            "version": "=" + version, "default-features": False, "features": matches[0]["features"],
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--target", default="x86_64-unknown-linux-gnu")
    parser.add_argument("--package", action="append", help="Refresh selected example packages plus the native base")
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    original = tomllib.loads((root / "Cargo.lock").read_text())
    output = root / "nix/cargo-projections"
    output.mkdir(exist_ok=True)
    native = output / "native.json"
    if not native.exists():
        write(native, {"lock": original})
    examples = [f"examples/{path.parent.parent.name}" for path in sorted(root.glob("examples/*/src/main.rs"))]
    catalog = evaluate(root, args.nixpkgs, f"""
      map (path: let directory = root + "/${{path}}";
                     manifest = builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"));
                 in {{ directory = path; package = manifest.package.name;
                      engine = sources.needsEngine directory; }})
        [ {" ".join(json.dumps(path) for path in examples + PACKAGED_CRATES)} ]
    """)
    if args.package:
        unknown = set(args.package) - {entry["package"] for entry in catalog} - {"lavashell-example-support"}
        if unknown:
            parser.error(f"unknown packages: {sorted(unknown)}")
        catalog = [entry for entry in catalog if entry["package"] in args.package]
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        workspace(root, args.nixpkgs, "examples/support", False, directory / "native")
        _, native_lock = graph(args, directory / "native", "lavashell-example-support", True)
        native_metadata = prepare_metadata(json.loads(native.read_text()), False, args.target) | {"lock": native_lock}
        write(native, native_metadata)
        store_dependency_plan(args, native_metadata, "registry", dependency_plan(
            args, root, "examples/support", False, directory / "native-layer",
        ))
        write(native, native_metadata)
        for entry in catalog:
            filename = output / f"{entry['package']}.json"
            previous = json.loads(filename.read_text()) if filename.exists() else {"normalLock": original}
            metadata = prepare_metadata(previous, entry["engine"], args.target)
            if entry["engine"]:
                metadata.setdefault("features", {})
                metadata.setdefault("engineLock", original)
            if not entry["engine"]:
                if not filename.exists():
                    write(filename, {"normalLock": original})
                normal = directory / entry["package"] / "normal"
                data = workspace(root, args.nixpkgs, entry["directory"], False, normal)
                _, normalized = graph(args, normal, entry["package"], any(
                    tomllib.loads(crate["manifest"])["package"]["name"] == "lavashell-renderer"
                    for crate in data["crates"]
                ))
                metadata["normalLock"] = normalized
                write(filename, metadata)
                store_dependency_plan(args, metadata, "registry", dependency_plan(
                    args, root, entry["directory"], False, directory / entry["package"] / "registry-layer",
                ))
                write(filename, metadata)
                continue
            metadata["features"] = {}
            native_names = evaluate(root, args.nixpkgs, "sources.nativeNames")
            metadata["engineLock"] = engine_seed_lock(original, metadata["engineLock"], native_names)
            write(filename, metadata)
            normal = directory / entry["package"] / "normal"
            data = workspace(root, args.nixpkgs, entry["directory"], False, normal)
            renderer = any(tomllib.loads(crate["manifest"])["package"]["name"] == "lavashell-renderer"
                           for crate in data["crates"])
            normal_graph, metadata["normalLock"] = graph(args, normal, entry["package"], renderer)
            write(filename, metadata)
            for attempt in itertools.count():
                engine = directory / entry["package"] / f"engine-{attempt}"
                workspace(root, args.nixpkgs, entry["directory"], True, engine)
                engine_graph, metadata["engineLock"] = graph(args, engine, entry["package"], False)
                extra = activations(engine_graph, normal_graph)
                if not extra:
                    metadata["externalUnits"] = plan.external_units(engine_graph)
                    plan.verify(metadata["externalUnits"], normal_graph)
                    write(filename, metadata)
                    store_dependency_plan(args, metadata, "engine", dependency_plan(
                        args, root, entry["directory"], True, directory / entry["package"] / "engine-layer",
                    ))
                    write(filename, metadata)
                    print(f"{entry['package']}: {len(metadata['externalUnits'])} matching external units")
                    break
                before = copy.deepcopy(metadata["features"])
                for kind, dependencies in extra.items():
                    metadata["features"].setdefault(kind, {}).update(dependencies)
                if metadata["features"] == before:
                    raise ValueError(f"shared features did not change for {entry['package']}")
                write(filename, metadata)
    print("Projection locks refreshed; no compilation performed")


if __name__ == "__main__":
    main()
