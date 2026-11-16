# SPDX-License-Identifier: GPL-3.0-only

import enum
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import ClassVar


class NextStep(enum.Enum):
    FIX_FETCH = enum.auto()
    CHECK_EXTERNALS = enum.auto()
    ENTER_SHELL = enum.auto()
    REPORT_DEFECT = enum.auto()

    @property
    def text(self) -> str:
        match self:
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
class CommandFailed(Cause):
    command: tuple[str, ...]
    stderr: str

    def message(self) -> str:
        return f"The command {' '.join(self.command)} failed:\n{self.stderr}"


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
