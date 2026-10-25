#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Plan the build of every projected workspace with Cargo in frozen mode and compare the plan with the stored
projection, without compiling anything."""

import argparse
import importlib.util
import json
import tempfile
import tomllib
from pathlib import Path

from flake_evaluation import locked_nixpkgs

spec = importlib.util.spec_from_file_location("projections", Path(__file__).with_name("refresh-cargo-projections.py"))
projections = importlib.util.module_from_spec(spec)
spec.loader.exec_module(projections)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--target", default="x86_64-unknown-linux-gnu")
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    entries = ["support"] + [directory.name for directory in sorted((root / "examples").iterdir())
                            if (directory / "src/main.rs").exists()]
    with tempfile.TemporaryDirectory() as temporary:
        for entry in entries:
            manifest = tomllib.loads((root / "examples" / entry / "Cargo.toml").read_text())
            package = manifest["package"]["name"]
            destination = Path(temporary) / entry
            data = projections.workspace(root, args.nixpkgs, f"examples/{entry}", False, destination)
            renderer = any(tomllib.loads(crate["manifest"])["package"]["name"] == "lavashell-renderer"
                           for crate in data["crates"])
            graph, _ = projections.graph(args, destination, package, renderer, frozen=True)
            metadata_file = root / f"nix/cargo-projections/{package}.json"
            dependency_file = root / "nix/cargo-projections/native.json" if entry == "support" else metadata_file
            if not dependency_file.exists():
                raise ValueError(f"missing canonical dependency metadata {dependency_file}; refresh projections")
            dependency_metadata = json.loads(dependency_file.read_text())
            layers = dependency_metadata.get("dependencyPlans", {}).get(args.target, {})
            expected_layer = "engine" if "engineLock" in dependency_metadata else "registry"
            if expected_layer not in layers:
                raise ValueError(f"missing canonical {expected_layer} plan in {dependency_file}; refresh projections")
            for layer, record in layers.items():
                if layer not in ("engine", "registry"):
                    raise ValueError(f"unknown dependency layer {layer}")
                projected = destination / f"canonical-{layer}"
                projections.projected_workspace(root, args.nixpkgs, f"examples/{entry}", layer, projected)
                selected = record["plan"]["selection"]
                canonical, _ = projections.graph(
                    args, projected, package, "lavashell-renderer" in selected["packages"], frozen=True,
                )
                if projections.plan.external_units(canonical) != record["plan"]["externalUnits"]:
                    raise AssertionError(f"canonical {layer} unit records differ for {package}")
                seed_directory = destination / f"seed-{layer}"
                projections.workspace(root, args.nixpkgs, f"examples/{entry}", layer == "engine", seed_directory)
                seed_graph, _ = projections.graph(
                    args, seed_directory, package, "lavashell-renderer" in selected["packages"], frozen=True,
                )
                projections.dependency.verify_units(seed_graph, canonical, projections.plan.external_units)
            if metadata_file.exists():
                metadata = json.loads(metadata_file.read_text())
                if "engineLock" not in metadata:
                    continue
                projections.plan.verify(metadata["externalUnits"], graph)
                engine = destination / "engine"
                projections.workspace(root, args.nixpkgs, f"examples/{entry}", True, engine)
                engine_graph, _ = projections.graph(args, engine, package, False, frozen=True)
                assert projections.plan.external_units(engine_graph) == metadata["externalUnits"]
                if metadata["features"]:
                    root_manifest = engine / f"examples/{entry}/Cargo.toml"
                    names = {name for dependencies in metadata["features"].values() for name in dependencies}
                    lines = root_manifest.read_text().splitlines()
                    root_manifest.write_text("\n".join(line for line in lines if not any(
                        line.startswith(json.dumps(name) + " = ") for name in names
                    )) + "\n")
                    unaligned, _ = projections.graph(args, engine, package, False)
                    try:
                        projections.plan.verify(projections.plan.external_units(unaligned), graph)
                    except ValueError:
                        pass
                    else:
                        raise AssertionError("missing shared feature activations were accepted")
    print(f"Passed {len(entries)} frozen Cargo workspace plans")


if __name__ == "__main__":
    main()
