#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Edit one source file, manifest or lock file at a time and evaluate the package derivations after each edit.

Each edit must change the derivations of the build layers that use the edited file and leave every other layer
unchanged.
"""

import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from flake_evaluation import locked_nixpkgs


def scope(module, root, nixpkgs, gui_tools=False):
    return f"""
      let
        original = import (builtins.toPath {json.dumps(nixpkgs)}) {{ system = "x86_64-linux"; }};
        pkgs = original // original.lib.optionalAttrs {str(gui_tools).lower()} {{
          fontconfig = original.fontconfig.overrideAttrs (old: {{ pname = old.pname + "-fixture"; }});
          freetype = original.freetype.overrideAttrs (old: {{ pname = old.pname + "-fixture"; }});
          clang = original.clang.overrideAttrs (old: {{ pname = old.pname + "-fixture"; }});
        }};
        tree = import (builtins.toPath {json.dumps(str(module))}) {{
          inherit pkgs;
          root = builtins.toPath {json.dumps(str(root))};
        }};
        reference = import (builtins.toPath {json.dumps(str(module.with_name("reference-package.nix")))}) {{
          inherit (original) lib;
          inherit tree;
        }};
      in
    """


def nix_eval(expression):
    result = subprocess.run(
        ["nix", "eval", "--offline", "--impure", "--json", "--expr", expression],
        text=True, capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def evaluate(module, root, nixpkgs, gui_tools=False, keys=None):
    keys = keys or ["registry", "framework", "engine", "referenceFramework", "reference", "native"]
    return nix_eval(scope(module, root, nixpkgs, gui_tools) + f"""
      original.lib.getAttrs [ {" ".join(json.dumps(key) for key in keys)} ] {{
        registry = tree.checks.registry.drvPath;
        registryOut = tree.checks.registry.outPath;
        framework = tree.checks.framework.drvPath;
        engine = reference.engineArtifacts.drvPath;
        engineOut = reference.engineArtifacts.outPath;
        referenceFramework = reference.frameworkArtifacts.drvPath;
        reference = reference.drvPath;
        native = tree.packages.interactions.drvPath;
      }}
    """)


def reference_package(module, root, nixpkgs):
    directory = Path(nix_eval(scope(module, root, nixpkgs) + "toString reference.directory"))
    return directory.relative_to(root)


def reference_source(root, directory):
    sources = root / directory / "src"
    return sources / "main.rs" if (sources / "main.rs").exists() else sources / "lib.rs"


def require_stale(module, root, nixpkgs):
    try:
        evaluate(module, root, nixpkgs)
    except RuntimeError as error:
        if "Stale dependency plan" in str(error):
            return
        raise
    raise AssertionError("dependency edit silently reused an unrefreshed projection")


def change(path, old, new):
    source = path.read_text()
    assert old in source, f"missing fixture text in {path}"
    path.write_text(source.replace(old, new, 1))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    module = root / "nix/examples.nix"
    results = {}
    with tempfile.TemporaryDirectory(prefix="boundaries-") as temporary:
        fixture = Path(temporary) / "repository"
        fixture.mkdir()
        for name in ("Cargo.toml", "Cargo.lock", "COPYING"):
            shutil.copy2(root / name, fixture / name)
        (fixture / "nix").mkdir()
        shutil.copytree(root / "nix/cargo-projections", fixture / "nix/cargo-projections")
        for name in ("crates", "examples", "tools"):
            shutil.copytree(root / name, fixture / name, ignore=shutil.ignore_patterns("node_modules", "target"))
        base = evaluate(module, fixture, args.nixpkgs)
        results["baseline"] = base
        directory = reference_package(module, fixture, args.nixpkgs)
        tools = evaluate(module, fixture, args.nixpkgs, gui_tools=True)
        assert tools["engine"] == base["engine"], "GUI tool/library changes reached the engine"
        assert all(tools[k] != base[k] for k in ("registry", "framework", "referenceFramework", "reference", "native"))
        results["gui_tools"] = tools
        source = reference_source(fixture, directory)
        old = source.read_text()
        source.write_text(old + "\n// source-only fixture edit\n")
        current = evaluate(module, fixture, args.nixpkgs)
        assert all(current[k] == base[k] for k in base if k != "reference"), "reference edit reached an earlier layer"
        assert current["reference"] != base["reference"]
        results["reference_source"] = current
        source.write_text(old)
        framework = fixture / "crates/lavashell-core/src/lib.rs"
        old = framework.read_text()
        framework.write_text(old + "\n// source-only fixture edit\n")
        current = evaluate(module, fixture, args.nixpkgs)
        assert current["registry"] == base["registry"] and current["engine"] == base["engine"]
        assert all(current[k] != base[k] for k in ("framework", "referenceFramework", "reference", "native"))
        results["framework_source"] = current
        framework.write_text(old)
        workspace = fixture / "Cargo.toml"
        original_workspace = workspace.read_text()
        lock = fixture / "Cargo.lock"
        original_lock = lock.read_text()
        foreign = fixture / "examples/foreign-fixture"
        (foreign / "src").mkdir(parents=True)
        (foreign / "src/main.rs").write_text("fn main() {}\n")
        (foreign / "Cargo.toml").write_text(
            '[package]\nname="foreign-fixture"\nversion="0.1.0"\nedition="2024"\n'
            '[dependencies]\nforeign-registry-fixture="=1.0.0"\n'
        )
        change(workspace, "members = [", 'members = ["examples/foreign-fixture",')
        lock.write_text(original_lock + '\n[[package]]\nname="foreign-fixture"\nversion="0.1.0"\n'
                        'dependencies=["foreign-registry-fixture"]\n'
                        '[[package]]\nname="foreign-registry-fixture"\nversion="1.0.0"\n'
                        'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                        f'checksum="{"0" * 64}"\n')
        current = evaluate(module, fixture, args.nixpkgs)
        assert current == base, "unrelated example reached existing package layers"
        results["unrelated_example"] = current
        change(lock, 'name="foreign-registry-fixture"\nversion="1.0.0"',
               'name="foreign-registry-fixture"\nversion="1.0.1"')
        current = evaluate(module, fixture, args.nixpkgs)
        assert current == base, "unrelated registry update reached existing package layers"
        results["unrelated_registry"] = current
        workspace.write_text(original_workspace)
        lock.write_text(original_lock)
        shutil.rmtree(foreign)
        change(workspace, 'zbus = { version = "=5.19.0"', 'zbus = { version = "=5.20.0"')
        change(lock, 'name = "zbus"\nversion = "5.19.0"', 'name = "zbus"\nversion = "5.20.0"')
        require_stale(module, fixture, args.nixpkgs)
        current = evaluate(module, fixture, args.nixpkgs, keys=["engine"])
        assert current["engine"] == base["engine"], "GUI-only registry update invalidated Bevy"
        results["gui_registry"] = current
        workspace.write_text(original_workspace)
        lock.write_text(original_lock)
        manifest = fixture / directory / "Cargo.toml"
        old = manifest.read_text()
        change(manifest, '"std",', '"bevy_scene", "std",')
        require_stale(module, fixture, args.nixpkgs)
        current = evaluate(module, fixture, args.nixpkgs, keys=["registry", "framework", "native"])
        assert all(current[k] == base[k] for k in current)
        results["engine_features"] = current
        manifest.write_text(old)
        bridge = fixture / "crates/lavashell-bevy/Cargo.toml"
        old_bridge = bridge.read_text()
        change(bridge, "version.workspace = true", 'version = "0.1.1"')
        change(lock, 'name = "lavashell-bevy"\nversion = "0.1.0"',
               'name = "lavashell-bevy"\nversion = "0.1.1"')
        current = evaluate(module, fixture, args.nixpkgs)
        assert all(current[k] == base[k] for k in ("registry", "framework", "engine", "native"))
        assert all(current[k] != base[k] for k in ("referenceFramework", "reference"))
        results["adapter_version"] = current
        bridge.write_text(old_bridge)
        lock.write_text(original_lock)
        change(workspace, 'bevy = { version = "=0.17.3"', 'bevy = { version = "=0.17.4"')
        change(lock, 'name = "bevy"\nversion = "0.17.3"', 'name = "bevy"\nversion = "0.17.4"')
        require_stale(module, fixture, args.nixpkgs)
        current = evaluate(module, fixture, args.nixpkgs, keys=["registry", "framework", "native"])
        assert all(current[k] == base[k] for k in current)
        results["engine_version"] = current
        for entry in results.values():
            assert all(path.endswith(".drv") for path in entry.values())
    print(f"Passed {len(results) - 1} derivation boundary regressions")


if __name__ == "__main__":
    main()
