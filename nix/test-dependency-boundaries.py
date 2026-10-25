#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Make edits that must not affect the dependency layers, refresh the Cargo projections after each edit, and compare
the selected external build units and the derivation paths of the layers with those of the unedited source. A change
of the build profile is the control case and must change every layer. Nothing is compiled.
"""

import argparse
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

from flake_evaluation import locked_nixpkgs


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


boundaries = module("layer_boundaries", "test-layer-boundaries.py")
projection = module("dependency_workspace", "dependency-workspace.py")
TARGET = "x86_64-unknown-linux-gnu"


def copy_fixture(source, destination):
    destination.mkdir()
    for filename in ("Cargo.toml", "Cargo.lock", "COPYING"):
        shutil.copy2(source / filename, destination / filename)
    for directory in ("nix", "crates", "examples", "tools"):
        shutil.copytree(source / directory, destination / directory,
                        ignore=shutil.ignore_patterns("target", "node_modules", ".cache", "__pycache__"))


def package_name(root, directory):
    return tomllib.loads((root / directory / "Cargo.toml").read_text())["package"]["name"]


def refresh(args, root, reference):
    command = [sys.executable, str(root / "nix/refresh-cargo-projections.py"), "--nixpkgs", args.nixpkgs,
               "--cargo", args.cargo, "--rustc", args.rustc, "--package", package_name(root, reference)]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)


def identity(args, root, reference):
    keys = boundaries.evaluate(root / "nix/examples.nix", root, args.nixpkgs,
                               keys=["registry", "registryOut", "engine", "engineOut"])
    units = {}
    for layer, filename in (("registry", "native"), ("engine", package_name(root, reference))):
        metadata = json.loads((root / f"nix/cargo-projections/{filename}.json").read_text())
        units[layer] = metadata["dependencyPlans"][TARGET][layer]["plan"]["externalUnits"]
    return keys, units


def rewrite_manifest(path, update):
    manifest = tomllib.loads(path.read_text())
    update(manifest)
    path.write_text(projection.manifest_text(manifest))


def target_dependencies(manifest, platform):
    return manifest.setdefault("target", {}).setdefault(platform, {}).setdefault("dependencies", {})


def move_feature(root, reference):
    def update(manifest):
        common = manifest["dependencies"]["bevy"]
        linux = target_dependencies(manifest, 'cfg(target_os = "linux")').setdefault(
            "bevy", {key: value for key, value in common.items() if key != "features"})
        linux_features = linux.setdefault("features", [])
        feature = next(feature for feature in common["features"] if feature not in linux_features)
        common["features"].remove(feature)
        linux_features.append(feature)
    rewrite_manifest(root / reference / "Cargo.toml", update)


def inactive_local(root, requester):
    name = "canonical-inactive-fixture"
    member = "crates/" + name
    crate = root / member
    (crate / "src").mkdir(parents=True)
    (crate / "src/lib.rs").write_text("\n")
    (crate / "build.rs").write_text("fn main() {}\n")
    (crate / "Cargo.toml").write_text(projection.manifest_text({
        "package": {"name": name, "version": "0.1.0", "edition": "2024", "links": name, "build": "build.rs"},
    }))
    rewrite_manifest(root / "Cargo.toml", lambda data: data["workspace"]["members"].append(member))

    def update(manifest):
        target_dependencies(manifest, 'cfg(target_os = "android")')[name] = {"path": "../../" + member}
    manifest_path = root / requester / "Cargo.toml"
    requester_name = tomllib.loads(manifest_path.read_text())["package"]["name"]
    rewrite_manifest(manifest_path, update)
    lock_path = root / "Cargo.lock"
    lock = tomllib.loads(lock_path.read_text())
    lock["package"].append({"name": name, "version": "0.1.0"})
    next(package for package in lock["package"] if package["name"] == requester_name)["dependencies"].append(name)
    lock_path.write_text(projection.lock_text(lock))


def inactive_native_local(root, _reference):
    inactive_local(root, "crates/lavashell")


def profile_change(root, _reference):
    def update(manifest):
        profile = manifest["profile"]["packaged"]
        profile["opt-level"] = 1 if profile.get("opt-level") != 1 else 2
    rewrite_manifest(root / "Cargo.toml", update)


def source_change(root, reference):
    source = boundaries.reference_source(root, reference)
    source.write_text(source.read_text() + "\n// canonical dependency cache source fixture\n")


def original_engine_links(root, reference):
    directory = root / reference
    rewrite_manifest(directory / "Cargo.toml",
                     lambda manifest: manifest["package"].update(links="canonical-active-fixture",
                                                                 build="custom-build.rs"))
    (directory / "custom-build.rs").write_text("fn main() {}\n")


def engine_build_script(root, reference):
    metadata = json.loads((root / f"nix/cargo-projections/{package_name(root, reference)}.json").read_text())
    crates = metadata["dependencyPlans"][TARGET]["engine"]["plan"]["crates"]
    return next(crate for crate in crates if crate["member"] == reference.as_posix())["manifest"]["package"]["build"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--rustc", default="rustc")
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="canonical-boundaries-") as temporary:
        base = Path(temporary) / "baseline"
        copy_fixture(root, base)
        reference = boundaries.reference_package(base / "nix/examples.nix", base, args.nixpkgs)
        refresh(args, base, reference)
        base_keys, base_units = identity(args, base, reference)
        for label, mutate, equivalent in (("application_source", source_change, True),
                                           ("common_to_active_target", move_feature, True),
                                           ("inactive_local_closure", inactive_local, True),
                                           ("inactive_native_closure", inactive_native_local, True),
                                           ("selected_profile", profile_change, False)):
            fixture = Path(temporary) / label
            copy_fixture(base, fixture)
            mutate(fixture, reference)
            if label != "application_source":
                refresh(args, fixture, reference)
            keys, units = identity(args, fixture, reference)
            if equivalent:
                assert units == base_units, f"{label}: selected external contexts changed"
                assert keys == base_keys, f"{label}: equivalent contexts changed dependency drv/output paths"
            else:
                assert all(units[layer] != base_units[layer] for layer in units), "profile control changed no contexts"
                assert all(keys[key] != base_keys[key] for key in keys), "profile control failed to invalidate a layer"
        fixture = Path(temporary) / "original_engine_links"
        copy_fixture(base, fixture)
        original_engine_links(fixture, reference)
        try:
            refresh(args, fixture, reference)
        except RuntimeError as error:
            if "unsupported original links contract" not in str(error):
                raise
        else:
            raise AssertionError("original_engine_links: normalized engine seed erased an unsupported links contract")
        fixture = Path(temporary) / "original_engine_implicit_build"
        copy_fixture(base, fixture)
        (fixture / reference / "build.rs").write_text("fn main() {}\n")
        refresh(args, fixture, reference)
        assert engine_build_script(fixture, reference) == "build.rs", "engine plan dropped the original build script"
    print("Passed 5 regenerated dependency-layer boundaries, 1 original-engine links rejection and 1 build script")


if __name__ == "__main__":
    main()
