#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Edit one source file, manifest or lock file in a copy of the repository per edit, and evaluate the package
derivations of each copy.

Each edit must change the derivations of the build layers that use the edited file and leave every other layer
unchanged.
"""

import argparse
import json
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from functools import partial
from pathlib import Path

from concurrent_checks import run_concurrently
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
          root = /. + {json.dumps(str(root))};
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


def copy_repository(root, destination):
    destination.mkdir()
    for name in ("Cargo.toml", "Cargo.lock", "COPYING", "about.toml"):
        shutil.copy2(root / name, destination / name)
    (destination / "nix").mkdir()
    shutil.copytree(root / "nix/cargo-projections", destination / "nix/cargo-projections")
    for name in ("crates", "examples", "tools"):
        shutil.copytree(root / name, destination / name, ignore=shutil.ignore_patterns("node_modules", "target"))


@dataclass(frozen=True)
class Baseline:
    module: Path
    nixpkgs: str
    repository: Path
    reference: Path
    derivations: dict

    def evaluate(self, fixture, **options):
        return evaluate(self.module, fixture, self.nixpkgs, **options)

    def require_stale(self, fixture):
        require_stale(self.module, fixture, self.nixpkgs)

    def fixture(self, label):
        fixture = self.repository.parent / label
        copy_repository(self.repository, fixture)
        return fixture


def gui_tools(baseline):
    base = baseline.derivations
    tools = baseline.evaluate(baseline.repository, gui_tools=True)
    assert tools["engine"] == base["engine"], "GUI tool/library changes reached the engine"
    assert all(tools[k] != base[k] for k in ("registry", "framework", "referenceFramework", "reference", "native"))
    return {"gui_tools": tools}


def reference_source_edit(baseline):
    base = baseline.derivations
    fixture = baseline.fixture("reference_source")
    source = reference_source(fixture, baseline.reference)
    source.write_text(source.read_text() + "\n// source-only fixture edit\n")
    current = baseline.evaluate(fixture)
    assert all(current[k] == base[k] for k in base if k != "reference"), "reference edit reached an earlier layer"
    assert current["reference"] != base["reference"]
    return {"reference_source": current}


def framework_source_edit(baseline):
    base = baseline.derivations
    fixture = baseline.fixture("framework_source")
    framework = fixture / "crates/lavashell-core/src/lib.rs"
    framework.write_text(framework.read_text() + "\n// source-only fixture edit\n")
    current = baseline.evaluate(fixture)
    assert current["registry"] == base["registry"] and current["engine"] == base["engine"]
    assert all(current[k] != base[k] for k in ("framework", "referenceFramework", "reference", "native"))
    return {"framework_source": current}


def unrelated_example(baseline):
    base = baseline.derivations
    fixture = baseline.fixture("unrelated_example")
    lock = fixture / "Cargo.lock"
    foreign = fixture / "examples/foreign-fixture"
    (foreign / "src").mkdir(parents=True)
    (foreign / "src/main.rs").write_text("fn main() {}\n")
    (foreign / "Cargo.toml").write_text(
        '[package]\nname="foreign-fixture"\nversion="0.1.0"\nedition="2024"\n'
        '[dependencies]\nforeign-registry-fixture="=1.0.0"\n'
    )
    change(fixture / "Cargo.toml", "members = [", 'members = ["examples/foreign-fixture",')
    lock.write_text(lock.read_text() + '\n[[package]]\nname="foreign-fixture"\nversion="0.1.0"\n'
                    'dependencies=["foreign-registry-fixture"]\n'
                    '[[package]]\nname="foreign-registry-fixture"\nversion="1.0.0"\n'
                    'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                    f'checksum="{"0" * 64}"\n')
    example = baseline.evaluate(fixture)
    assert example == base, "unrelated example reached existing package layers"
    change(lock, 'name="foreign-registry-fixture"\nversion="1.0.0"',
           'name="foreign-registry-fixture"\nversion="1.0.1"')
    registry = baseline.evaluate(fixture)
    assert registry == base, "unrelated registry update reached existing package layers"
    return {"unrelated_example": example, "unrelated_registry": registry}


def gui_registry_update(baseline):
    fixture = baseline.fixture("gui_registry")
    change(fixture / "Cargo.toml", 'zbus = { version = "=5.19.0"', 'zbus = { version = "=5.20.0"')
    change(fixture / "Cargo.lock", 'name = "zbus"\nversion = "5.19.0"', 'name = "zbus"\nversion = "5.20.0"')
    baseline.require_stale(fixture)
    current = baseline.evaluate(fixture, keys=["engine"])
    assert current["engine"] == baseline.derivations["engine"], "GUI-only registry update invalidated Bevy"
    return {"gui_registry": current}


def engine_features(baseline):
    fixture = baseline.fixture("engine_features")
    change(fixture / baseline.reference / "Cargo.toml", '"std",', '"bevy_scene", "std",')
    baseline.require_stale(fixture)
    current = baseline.evaluate(fixture, keys=["registry", "framework", "native"])
    assert all(current[k] == baseline.derivations[k] for k in current)
    return {"engine_features": current}


def adapter_version(baseline):
    base = baseline.derivations
    fixture = baseline.fixture("adapter_version")
    change(fixture / "crates/lavashell-bevy/Cargo.toml", "version.workspace = true", 'version = "0.1.1"')
    change(fixture / "Cargo.lock", 'name = "lavashell-bevy"\nversion = "0.1.0"',
           'name = "lavashell-bevy"\nversion = "0.1.1"')
    current = baseline.evaluate(fixture)
    assert all(current[k] == base[k] for k in ("registry", "framework", "engine", "native"))
    assert all(current[k] != base[k] for k in ("referenceFramework", "reference"))
    return {"adapter_version": current}


def engine_version(baseline):
    fixture = baseline.fixture("engine_version")
    change(fixture / "Cargo.toml", 'bevy = { version = "=0.17.3"', 'bevy = { version = "=0.17.4"')
    change(fixture / "Cargo.lock", 'name = "bevy"\nversion = "0.17.3"', 'name = "bevy"\nversion = "0.17.4"')
    baseline.require_stale(fixture)
    current = baseline.evaluate(fixture, keys=["registry", "framework", "native"])
    assert all(current[k] == baseline.derivations[k] for k in current)
    return {"engine_version": current}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    module = root / "nix/examples.nix"
    with tempfile.TemporaryDirectory(prefix="boundaries-") as temporary:
        repository = Path(temporary) / "repository"
        copy_repository(root, repository)
        base = evaluate(module, repository, args.nixpkgs)
        reference = reference_package(module, repository, args.nixpkgs)
        baseline = Baseline(module, args.nixpkgs, repository, reference, base)
        checks = (gui_tools, reference_source_edit, framework_source_edit, unrelated_example, gui_registry_update,
                  engine_features, adapter_version, engine_version)
        results = {"baseline": base}
        for result in run_concurrently([partial(check, baseline) for check in checks]):
            results.update(result)
        for entry in results.values():
            assert all(path.endswith(".drv") for path in entry.values())
    print(f"Passed {len(results) - 1} derivation boundary regressions")


if __name__ == "__main__":
    main()
