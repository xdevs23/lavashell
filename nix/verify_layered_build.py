#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import argparse
import enum
import io
import json
import subprocess
import tarfile
import tempfile
import time
from collections.abc import Sequence
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Self

import check_cargo_reuse as reuse
from cargo_files import package_name
from document_tables import DocumentTable
from nix_evaluation import applied_expression, nix_eval, path_text
from projection_contract import CONTRACT
from repository_paths import NIX_DIRECTORY, REPOSITORY_ROOT
from verification_errors import (DirtyCheckout, EditNotCompiled, LayerInvalidated, ReferenceUnchanged,
                                 SandboxBuildFailed, SourceChangedDuringValidation, VerificationError)


def run(command: list[str], cwd: Path | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, check=True, text=True, capture_output=True)
    return result.stdout.strip()


REFERENCE_PACKAGE = "import ./reference-package.nix"

REFERENCE_DERIVATIONS = """
  arguments:
  let
    reference = import ./reference-package.nix arguments;
  in
  {
    reference = reference.drvPath;
    engine = reference.engineArtifacts.drvPath;
    registry = reference.registryArtifacts.drvPath;
    framework = reference.frameworkArtifacts.drvPath;
    engineOut = reference.engineArtifacts.outPath;
    frameworkOut = reference.frameworkArtifacts.outPath;
    referenceOut = reference.outPath;
    directory = toString reference.directory;
  }
"""


class Phase(enum.Enum):
    BASELINE_BUILD = "baseline_build"
    BASELINE_FRESHNESS_VERIFICATION = "baseline_freshness_verification"
    BASELINE_FRESHNESS_VERIFIED = "baseline_freshness_verified"
    SOURCE_EDIT_BUILD = "source_edit_build"
    FRESHNESS_VERIFICATION = "freshness_verification"
    COMPLETE = "complete"


@dataclass(frozen=True)
class SourceArguments:
    root: str


@dataclass(frozen=True)
class ReferenceDerivations:
    reference: str
    engine: str
    registry: str
    framework: str
    engine_out: Path
    framework_out: Path
    reference_out: Path
    directory: Path

    @classmethod
    def from_json(cls, raw: DocumentTable) -> Self:
        return cls(
            reference=raw.string("reference"),
            engine=raw.string("engine"),
            registry=raw.string("registry"),
            framework=raw.string("framework"),
            engine_out=Path(raw.string("engineOut")),
            framework_out=Path(raw.string("frameworkOut")),
            reference_out=Path(raw.string("referenceOut")),
            directory=Path(raw.string("directory")),
        )

    def layers(self) -> dict[str, str]:
        return {"engine": self.engine, "registry": self.registry, "framework": self.framework}


@dataclass
class Stage:
    source: str
    derivations: ReferenceDerivations
    command: list[str]
    wall_seconds: float | None = None
    file: str | None = None


@dataclass(frozen=True)
class BaselineReuse:
    fresh_engine_in_framework: int
    fresh_engine_in_baseline: int


@dataclass(frozen=True)
class EditReuse:
    fresh_engine_after_edit: int
    fresh_dependencies_after_edit: int


@dataclass
class Report:
    commit: str
    phase: Phase | None = None
    complete: bool = False
    stages: dict[str, Stage] = field(default_factory=dict)
    baseline_reuse: BaselineReuse | None = None
    edit_reuse: EditReuse | None = None

    def document(self) -> dict[str, object]:
        reuse_counts = {**(asdict(self.baseline_reuse) if self.baseline_reuse else {}),
                        **(asdict(self.edit_reuse) if self.edit_reuse else {})}
        return {
            "commit": self.commit,
            "complete": self.complete,
            "phase": None if self.phase is None else self.phase.value,
            **{name: asdict(stage) for name, stage in self.stages.items()},
            **reuse_counts,
        }


def evaluate(source: str) -> ReferenceDerivations:
    derivations = nix_eval(REFERENCE_DERIVATIONS, SourceArguments(source))
    return ReferenceDerivations.from_json(DocumentTable.decoded(derivations, "reference derivations"))


def snapshot(directory: Path) -> str:
    return run(["nix", "store", "add", "--name", "source", str(directory)])


def build_command(source: str) -> list[str]:
    return ["nix", "build", "--offline", "--impure", "--no-link", "--json", "-L", "--option", "sandbox", "true",
            *applied_expression(REFERENCE_PACKAGE, SourceArguments(source))]


def progress(output: Path, report: Report, phase: Phase) -> None:
    report.phase = phase
    temporary = output / "report.json.tmp"
    temporary.write_text(json.dumps(report.document(), indent=2, default=path_text) + "\n")
    temporary.replace(output / "report.json")


def build(source: str, log: Path) -> float:
    command = build_command(source)
    with log.open("w") as output:
        started = time.monotonic()
        result = subprocess.run(command, cwd=NIX_DIRECTORY, stdout=output, stderr=subprocess.STDOUT)
        wall_seconds = time.monotonic() - started
    if result.returncode:
        raise VerificationError(SandboxBuildFailed(code=result.returncode, log=log))
    return wall_seconds


def artifact_path(output: Path) -> Path:
    return output / "share/lavashell/cargo-artifacts.jsonl"


def build_stage(output: Path, report: Report, name: str, source: str, derivations: ReferenceDerivations,
                phase: Phase) -> Stage:
    stage = report.stages[name] = Stage(source, derivations, build_command(source))
    progress(output, report, phase)
    stage.wall_seconds = build(source, output / f"{name}.log")
    return stage


def extract_commit(root: Path, commit: str, directory: Path) -> None:
    archive = subprocess.run(["git", "archive", commit], cwd=root, check=True, capture_output=True).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        source.extractall(directory, filter="data")


def edit_reference_source(crate: Path) -> Path:
    source = crate / "src/main.rs" if (crate / "src/main.rs").exists() else crate / "src/lib.rs"
    source.write_text(source.read_text() + "\n// Nix compilation-layer source-edit verification.\n")
    return source


def verify_baseline_reuse(engine: Path, framework: Path, original: Path) -> BaselineReuse:
    reference = reuse.records(engine)
    return BaselineReuse(
        fresh_engine_in_framework=reuse.verify(reference, reuse.records(framework), CONTRACT.engine_root, None),
        fresh_engine_in_baseline=reuse.verify(reference, reuse.records(original), CONTRACT.engine_root, None),
    )


def verify_layers_after_edit(baseline: ReferenceDerivations, changed: ReferenceDerivations) -> None:
    for name, derivation in changed.layers().items():
        if derivation != baseline.layers()[name]:
            raise VerificationError(LayerInvalidated(layer=name))
    if changed.reference == baseline.reference:
        raise VerificationError(ReferenceUnchanged())


def verify_edit_reuse(engine: Path, original: Path, changed: Path, package: str) -> EditReuse:
    changed_records = reuse.records(changed)
    verified = EditReuse(
        fresh_engine_after_edit=reuse.verify(reuse.records(engine), changed_records, CONTRACT.engine_root, None),
        fresh_dependencies_after_edit=reuse.verify(reuse.records(original), changed_records, "", package),
    )
    own = [record for record in changed_records if record.package_name == package]
    if not own or all(record.fresh for record in own):
        raise VerificationError(EditNotCompiled())
    return verified


def main(argv: Sequence[str] | None = None) -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    output: Path = parser.parse_args(argv).output
    root = REPOSITORY_ROOT
    if run(["git", "status", "--porcelain"], cwd=root):
        raise VerificationError(DirtyCheckout())
    commit = run(["git", "rev-parse", "HEAD"], cwd=root)
    output.mkdir(parents=True, exist_ok=True)
    report = Report(commit)
    with tempfile.TemporaryDirectory(prefix="sources-", dir=output) as temporary:
        directory = Path(temporary)
        extract_commit(root, commit, directory)
        baseline_source = snapshot(directory)
        baseline = evaluate(baseline_source)
        build_stage(output, report, "baseline", baseline_source, baseline, Phase.BASELINE_BUILD)
        engine = artifact_path(baseline.engine_out)
        framework = artifact_path(baseline.framework_out)
        original_reference = artifact_path(baseline.reference_out)
        progress(output, report, Phase.BASELINE_FRESHNESS_VERIFICATION)
        report.baseline_reuse = verify_baseline_reuse(engine, framework, original_reference)
        progress(output, report, Phase.BASELINE_FRESHNESS_VERIFIED)
        crate = directory / baseline.directory.relative_to(baseline_source)
        package = package_name(crate)
        source = edit_reference_source(crate)
        changed_source = snapshot(directory)
        changed = evaluate(changed_source)
        verify_layers_after_edit(baseline, changed)
        stage = build_stage(output, report, "changed", changed_source, changed, Phase.SOURCE_EDIT_BUILD)
        stage.file = str(source.relative_to(directory))
        progress(output, report, Phase.FRESHNESS_VERIFICATION)
        report.edit_reuse = verify_edit_reuse(engine, original_reference, artifact_path(changed.reference_out), package)
    if run(["git", "rev-parse", "HEAD"], cwd=root) != commit or run(["git", "status", "--porcelain"], cwd=root):
        raise VerificationError(SourceChangedDuringValidation())
    report.complete = True
    progress(output, report, Phase.COMPLETE)
    print("Sandbox builds and source-edit Cargo reuse passed")


if __name__ == "__main__":
    main()
