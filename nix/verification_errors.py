# SPDX-License-Identifier: GPL-3.0-only

import enum
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar


class NextStep(enum.Enum):
    REFRESH = enum.auto()
    CHECK_MANIFESTS = enum.auto()
    MODEL_INPUT = enum.auto()
    REBUILD = enum.auto()
    FIX_CARGO = enum.auto()
    FIX_BUILD = enum.auto()
    COMMIT_CHANGES = enum.auto()
    CHECK_LAYERS = enum.auto()
    DISCARD = enum.auto()
    FIX_FETCH = enum.auto()
    CHECK_EXTERNALS = enum.auto()
    ENTER_SHELL = enum.auto()
    REPORT_DEFECT = enum.auto()

    @property
    def text(self) -> str:
        match self:
            case NextStep.REFRESH:
                return "Run nix/refresh_cargo_projections.py and commit the refreshed projections."
            case NextStep.CHECK_MANIFESTS:
                return ("Check the Cargo manifests of the selected packages, "
                        "then run nix/refresh_cargo_projections.py again.")
            case NextStep.MODEL_INPUT:
                return "Remove it from the crates built in layers, or extend nix/dependency_workspace.py to model it."
            case NextStep.REBUILD:
                return "Run the build again with its Cargo JSON output kept, then verify again."
            case NextStep.FIX_CARGO:
                return "Fix the Cargo error, then run the refresh again."
            case NextStep.FIX_BUILD:
                return "Read the log, fix the build error, then run the validation again."
            case NextStep.COMMIT_CHANGES:
                return "Commit or discard the changes in the checkout, then run the validation again."
            case NextStep.CHECK_LAYERS:
                return "Check how nix/artifacts.nix splits the layers and what each layer takes as input."
            case NextStep.DISCARD:
                return "Do not publish this projection. Check the Cargo output of the refresh."
            case NextStep.FIX_FETCH:
                return "Check the network and the command, then run the refresh again."
            case NextStep.CHECK_EXTERNALS:
                return "Check the externals in nix/skia-licenses.json against the DEPS file of the Skia tag."
            case NextStep.ENTER_SHELL:
                return "Run the command inside the development shell of the flake."
            case NextStep.REPORT_DEFECT:
                return "Cargo or Nix wrote output that this tool cannot read. Report this output to the developers."


class Cause(ABC):
    next_step: ClassVar[NextStep]

    @abstractmethod
    def message(self) -> str: ...


class VerificationError(Exception):
    def __init__(self, cause: Cause) -> None:
        super().__init__(cause)
        self.cause = cause

    def __str__(self) -> str:
        return f"{self.cause.message()} {self.cause.next_step.text}"


@dataclass(frozen=True)
class EngineUnitsChanged(Cause):
    count: int
    next_step = NextStep.REFRESH

    def message(self) -> str:
        return f"{self.count} engine units changed."


@dataclass(frozen=True)
class EngineUnitsUncached(Cause):
    count: int
    next_step = NextStep.REFRESH

    def message(self) -> str:
        return f"{self.count} engine units are not cached."


@dataclass(frozen=True)
class ExternalContextsChanged(Cause):
    missing: int
    extra: int
    next_step = NextStep.DISCARD

    def message(self) -> str:
        return (f"The canonical dependency plan lost {self.missing} external contexts "
                f"and added {self.extra} others.")


@dataclass(frozen=True)
class ExternalPathPackage(Cause):
    package: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return (f"The path package {self.package} is outside the workspace "
                "and cannot be replaced by a workspace stub.")


@dataclass(frozen=True)
class NonVirtualWorkspace(Cause):
    member: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return (f"The member {self.member} is not a contained path of a virtual workspace, "
                "which dependency projections need.")


@dataclass(frozen=True)
class UnsupportedEdgeFlags(Cause):
    parent: str
    child: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return (f"The dependency edge {self.parent} -> {self.child} "
                "sets flags that a dependency layer cannot reproduce.")


@dataclass(frozen=True)
class UnsupportedSource(Cause):
    source: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return (f"The declared source {self.source} is not supported, "
                "because registry and git configuration are not inferred.")


@dataclass(frozen=True)
class OriginalLinks(Cause):
    member: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return f"The selected engine member {self.member} has an original links contract."


@dataclass(frozen=True)
class OriginalBuildScript(Cause):
    member: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return (f"The selected engine member {self.member} "
                "has an original build script that its seed did not follow.")


@dataclass(frozen=True)
class PathPatch(Cause):
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return "Path patches and replacements need real source handling, not dummy dependency stubs."


@dataclass(frozen=True)
class ExternalDependsOnLocal(Cause):
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return "External units depend on local source, and a dummy layer cannot preserve that input."


@dataclass(frozen=True)
class LocalLinks(Cause):
    package: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return f"The local package {self.package} links native code, which an empty stub cannot replace."


@dataclass(frozen=True)
class UnsupportedMode(Cause):
    mode: str
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return f"A dependency layer does not support the Cargo unit mode {self.mode}."


@dataclass(frozen=True)
class UnsupportedTargetKind(Cause):
    kinds: tuple[str, ...]
    next_step = NextStep.MODEL_INPUT

    def message(self) -> str:
        return f"A dependency layer does not support the local target kind {', '.join(self.kinds)}."


@dataclass(frozen=True)
class InvalidUnitIndex(Cause):
    index: int
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return f"Cargo reported the invalid unit index {self.index}."


@dataclass(frozen=True)
class NoUnitsSelected(Cause):
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return "Cargo selected no units."


@dataclass(frozen=True)
class PackageIdentityMismatch(Cause):
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return "Cargo metadata and the unit graph disagree on package identities."


@dataclass(frozen=True)
class SharedMember(Cause):
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return "Several resolved packages occupy one workspace member."


@dataclass(frozen=True)
class AmbiguousAlias(Cause):
    crate_name: str
    package: str
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return f"The dependency alias of {self.crate_name} in {self.package} is ambiguous or missing."


@dataclass(frozen=True)
class ConflictingRequests(Cause):
    package: str
    table: str
    alias: str
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return (f"Distinct host and target requests share {self.package}.{self.table}.{self.alias}, "
                "and one manifest cannot merge them.")


@dataclass(frozen=True)
class RootPackageMissing(Cause):
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return "The selected root package is missing from the active local closure."


@dataclass(frozen=True)
class AmbiguousFeatureContext(Cause):
    package: str
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return f"The feature context of {self.package} is ambiguous."


@dataclass(frozen=True)
class SharedFeaturesUnchanged(Cause):
    package: str
    next_step = NextStep.CHECK_MANIFESTS

    def message(self) -> str:
        return f"The shared features of {self.package} did not change in the last round."


@dataclass(frozen=True)
class CommandFailed(Cause):
    command: tuple[str, ...]
    stderr: str

    def message(self) -> str:
        return f"The command {' '.join(self.command)} failed:\n{self.stderr}"


@dataclass(frozen=True)
class CargoFailed(CommandFailed):
    next_step = NextStep.FIX_CARGO


@dataclass(frozen=True)
class FetchFailed(CommandFailed):
    next_step = NextStep.FIX_FETCH


@dataclass(frozen=True)
class NixpkgsUnset(Cause):
    next_step = NextStep.ENTER_SHELL

    def message(self) -> str:
        return "The environment variable LAVASHELL_NIXPKGS is not set."


@dataclass(frozen=True)
class SkiaDepsUnreadable(Cause):
    next_step = NextStep.CHECK_EXTERNALS

    def message(self) -> str:
        return "The DEPS file of Skia has no literal deps table."


@dataclass(frozen=True)
class SkiaExternalUnreadable(Cause):
    name: str
    next_step = NextStep.CHECK_EXTERNALS

    def message(self) -> str:
        return f"The DEPS file of Skia has no literal url and revision for the external {self.name}."


@dataclass(frozen=True)
class SandboxBuildFailed(Cause):
    code: int
    log: Path
    next_step = NextStep.FIX_BUILD

    def message(self) -> str:
        return f"The sandbox build failed with exit code {self.code}. Its log is {self.log}."


@dataclass(frozen=True)
class DirtyCheckout(Cause):
    next_step = NextStep.COMMIT_CHANGES

    def message(self) -> str:
        return "Layer validation needs a clean committed checkout."


@dataclass(frozen=True)
class SourceChangedDuringValidation(Cause):
    next_step = NextStep.COMMIT_CHANGES

    def message(self) -> str:
        return "The source changed during the validation."


@dataclass(frozen=True)
class LayerInvalidated(Cause):
    layer: str
    next_step = NextStep.CHECK_LAYERS

    def message(self) -> str:
        return f"A source edit invalidated the {self.layer} layer."


@dataclass(frozen=True)
class ReferenceUnchanged(Cause):
    next_step = NextStep.CHECK_LAYERS

    def message(self) -> str:
        return "A source edit did not change the derivation of the final reference package."


@dataclass(frozen=True)
class EditNotCompiled(Cause):
    next_step = NextStep.CHECK_LAYERS

    def message(self) -> str:
        return "The build of the changed reference package did not compile the edited crate."


@dataclass(frozen=True)
class BuildNotFinished(Cause):
    path: Path
    next_step = NextStep.REBUILD

    def message(self) -> str:
        return f"The file {self.path} has no successful Cargo build result."


@dataclass(frozen=True)
class NoReferenceArtifacts(Cause):
    prefix: str
    next_step = NextStep.REBUILD

    def message(self) -> str:
        return f"No reference compiler artifact matches the prefix {self.prefix}."


@dataclass(frozen=True)
class ArtifactIdentitiesChanged(Cause):
    next_step = NextStep.REBUILD

    def message(self) -> str:
        return "Dependency features, profiles or artifact identities changed between the builds."


@dataclass(frozen=True)
class DependencyRebuilt(Cause):
    next_step = NextStep.REBUILD

    def message(self) -> str:
        return "Cargo rebuilt a dependency that it should have reused."
