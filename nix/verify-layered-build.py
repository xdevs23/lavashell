#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Build the reference package in the Nix sandbox from a copy of the committed source, then again after an edit.

The edit appends a comment to one source file of the reference package. It must leave the engine, registry and
framework layers unchanged, and Cargo must reuse their compiled artifacts and recompile only the edited crate.
"""

import argparse
import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import time
import tomllib
from pathlib import Path

from flake_evaluation import locked_nixpkgs


def run(command, *, cwd=None):
    result = subprocess.run(command, cwd=cwd, check=True, text=True, capture_output=True)
    return result.stdout.strip()


def expression(source, nixpkgs):
    return f"""
      let
        root = builtins.toPath {json.dumps(source)};
        pkgs = import (builtins.toPath {json.dumps(nixpkgs)}) {{ system = "x86_64-linux"; }};
        tree = import (root + "/nix/examples.nix") {{ inherit pkgs root; }};
      in import (root + "/nix/reference-package.nix") {{ inherit (pkgs) lib; inherit tree; }}
    """


def evaluate(source, nixpkgs):
    package = expression(source, nixpkgs)
    query = f"""
      let reference = {package};
      in {{ reference = reference.drvPath; engine = reference.engineArtifacts.drvPath;
           registry = reference.registryArtifacts.drvPath; framework = reference.frameworkArtifacts.drvPath;
           engineOut = reference.engineArtifacts.outPath; frameworkOut = reference.frameworkArtifacts.outPath;
           referenceOut = reference.outPath; directory = toString reference.directory; }}
    """
    return json.loads(run(["nix", "eval", "--offline", "--impure", "--json", "--expr", query]))


def snapshot(directory):
    return run(["nix", "eval", "--offline", "--impure", "--raw", "--expr",
                f"builtins.path {{ path = builtins.toPath {json.dumps(str(directory))}; name = \"source\"; }}"])


def build_command(source, nixpkgs):
    return ["nix", "build", "--offline", "--impure", "--no-link", "--json", "-L",
            "--option", "sandbox", "true",
            "--expr", expression(source, nixpkgs)]


def progress(output, report, phase):
    report["phase"] = phase
    temporary = output / "report.json.tmp"
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(output / "report.json")


def build(source, nixpkgs, log):
    command = build_command(source, nixpkgs)
    with log.open("w") as output:
        started = time.monotonic()
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT)
        wall_seconds = time.monotonic() - started
    if result.returncode:
        raise RuntimeError(f"sandbox build failed with {result.returncode}: {log}")
    return wall_seconds


def artifact_path(output):
    return Path(output) / "share/lavashell/cargo-artifacts.jsonl"


def verify_baseline_reuse(engine, framework, package, reuse):
    reference = reuse.records(engine)
    return {
        "fresh_bevy_in_framework": reuse.verify(reference, reuse.records(framework), "bevy"),
        "fresh_bevy_in_baseline": reuse.verify(reference, reuse.records(package), "bevy"),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nixpkgs", help="the nixpkgs source (default: the one the flake locks)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.nixpkgs = args.nixpkgs or locked_nixpkgs()
    root = Path(__file__).resolve().parents[1]
    if run(["git", "status", "--porcelain"], cwd=root):
        raise RuntimeError("layer validation requires a clean committed checkout")
    commit = run(["git", "rev-parse", "HEAD"], cwd=root)
    args.output.mkdir(parents=True, exist_ok=True)
    report = {"commit": commit, "complete": False}
    archive = subprocess.run(["git", "archive", commit], cwd=root, check=True, capture_output=True).stdout
    with tempfile.TemporaryDirectory(prefix="sources-", dir=args.output) as temporary:
        directory = Path(temporary)
        with tarfile.open(fileobj=io.BytesIO(archive)) as source:
            source.extractall(directory, filter="data")
        baseline_source = snapshot(directory)
        baseline = evaluate(baseline_source, args.nixpkgs)
        report["baseline"] = {"source": baseline_source, "derivations": baseline}
        report["baseline"]["command"] = build_command(baseline_source, args.nixpkgs)
        progress(args.output, report, "baseline_build")
        report["baseline"]["wall_seconds"] = build(baseline_source, args.nixpkgs, args.output / "baseline.log")
        module = Path(baseline_source) / "nix/check-cargo-reuse.py"
        spec = importlib.util.spec_from_file_location("reuse", module)
        reuse = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reuse)
        engine = artifact_path(baseline["engineOut"])
        framework = artifact_path(baseline["frameworkOut"])
        original_reference = artifact_path(baseline["referenceOut"])
        progress(args.output, report, "baseline_freshness_verification")
        report.update(verify_baseline_reuse(engine, framework, original_reference, reuse))
        progress(args.output, report, "baseline_freshness_verified")
        crate = directory / Path(baseline["directory"]).relative_to(baseline_source)
        package = tomllib.loads((crate / "Cargo.toml").read_text())["package"]["name"]
        source = crate / "src/main.rs" if (crate / "src/main.rs").exists() else crate / "src/lib.rs"
        source.write_text(source.read_text() + "\n// Nix compilation-layer source-edit verification.\n")
        changed_source = snapshot(directory)
        changed = evaluate(changed_source, args.nixpkgs)
        for name in ("engine", "registry", "framework"):
            if changed[name] != baseline[name]:
                raise AssertionError(f"source edit invalidated {name}")
        if changed["reference"] == baseline["reference"]:
            raise AssertionError("source edit did not change the final reference derivation")
        report["changed"] = {"source": changed_source, "derivations": changed,
                             "file": str(source.relative_to(directory))}
        report["changed"]["command"] = build_command(changed_source, args.nixpkgs)
        progress(args.output, report, "source_edit_build")
        report["changed"]["wall_seconds"] = build(changed_source, args.nixpkgs, args.output / "changed.log")
        progress(args.output, report, "freshness_verification")
        changed_reference = artifact_path(changed["referenceOut"])
        engine_records = reuse.records(engine)
        original_records = reuse.records(original_reference)
        changed_records = reuse.records(changed_reference)
        report["fresh_bevy_after_edit"] = reuse.verify(engine_records, changed_records, "bevy")
        report["fresh_dependencies_after_edit"] = reuse.verify(original_records, changed_records, "", package)
        own = [record for record in changed_records if reuse.package_name(record) == package]
        if not own or all(record["fresh"] for record in own):
            raise AssertionError("changed reference compilation was not observed")
    if run(["git", "rev-parse", "HEAD"], cwd=root) != commit or run(["git", "status", "--porcelain"], cwd=root):
        raise RuntimeError("source changed during validation")
    report["complete"] = True
    progress(args.output, report, "complete")
    print("Sandbox builds and source-edit Cargo reuse passed")


if __name__ == "__main__":
    main()
