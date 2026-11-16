# SPDX-License-Identifier: GPL-3.0-only

import enum
import sys
from typing import NoReturn


class NextStep(enum.Enum):
    ENTER_NATIVE_SHELL = enum.auto()
    READ_RUN_LOGS = enum.auto()
    FIX_APP_DECLARATION = enum.auto()
    BUILD_PACKAGE = enum.auto()
    FIX_ARGUMENTS = enum.auto()
    REPLACE_SIGNING_KEY = enum.auto()
    FIX_LISTED_PROBLEMS = enum.auto()
    FIX_CHECK_ERROR = enum.auto()
    REPORT_DEFECT = enum.auto()

    @property
    def text(self) -> str:
        match self:
            case NextStep.ENTER_NATIVE_SHELL:
                return "Run the tool inside `nix develop .#native`, which provides the compositor and grim."
            case NextStep.READ_RUN_LOGS:
                return "Read the logs of the run, fix the cause named there, then run the tool again."
            case NextStep.FIX_APP_DECLARATION:
                return "Correct the declaration of the app in the Nix files of the repository, then run the tool again."
            case NextStep.BUILD_PACKAGE:
                return "Build the package named above, then run the tool again."
            case NextStep.FIX_ARGUMENTS:
                return "Correct the arguments of the command, then run it again."
            case NextStep.REPLACE_SIGNING_KEY:
                return (
                    "Delete the files of the development signing key in the signing directory, "
                    "so that the tool creates a new key, then run it again."
                )
            case NextStep.FIX_LISTED_PROBLEMS:
                return "Fix each listed problem, then run the check again."
            case NextStep.FIX_CHECK_ERROR:
                return "The check could not run. Fix the cause named in the error, then run the check again."
            case NextStep.REPORT_DEFECT:
                return "The tool or a program it ran is defective. Report this output to the developers of the project."


class ToolError(Exception):
    def __init__(self, message: str, next_step: NextStep) -> None:
        super().__init__(message)
        self.next_step = next_step


def exit_with_next_step(error: ToolError) -> NoReturn:
    sys.exit(f"{error}\n{error.next_step.text}")


def exit_with_check_result(problems: list[str], passed: str) -> NoReturn:
    if problems:
        print("\n".join([*problems, NextStep.FIX_LISTED_PROBLEMS.text]))
        sys.exit(1)
    print(passed)
    sys.exit(0)
