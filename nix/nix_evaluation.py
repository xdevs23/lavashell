# SPDX-License-Identifier: GPL-3.0-only

import json
import os
import subprocess
from dataclasses import asdict
from pathlib import Path, PurePath
from typing import TYPE_CHECKING

from repository_paths import NIX_DIRECTORY
from verification_errors import NixpkgsUnset, VerificationError

if TYPE_CHECKING:
    from _typeshed import DataclassInstance


class NixEvaluationError(Exception):
    def __init__(self, stderr: str) -> None:
        super().__init__(stderr)
        self.stderr = stderr


def nixpkgs_source() -> Path:
    source = os.environ.get("LAVASHELL_NIXPKGS")
    if source is None:
        raise VerificationError(NixpkgsUnset())
    return Path(source)


def path_text(value: object) -> str:
    if isinstance(value, PurePath):
        return str(value)
    raise TypeError(f"{type(value).__name__} is not a value that Nix arguments can contain")


def nix_attribute_name(field: str) -> str:
    first, *rest = field.split("_")
    return first + "".join(word.capitalize() for word in rest)


def nix_attributes(fields: list[tuple[str, object]]) -> dict[str, object]:
    return {nix_attribute_name(name): value for name, value in fields}


def applied_expression(function: str, arguments: "DataclassInstance") -> list[str]:
    document = {"nixpkgs": nixpkgs_source(), **asdict(arguments, dict_factory=nix_attributes)}
    return ["--expr", f"{{ arguments }}: ({function}) (builtins.fromJSON arguments)",
            "--argstr", "arguments", json.dumps(document, default=path_text)]


def nix_eval(function: str, arguments: "DataclassInstance") -> object:
    command = ["nix-instantiate", "--eval", "--strict", "--json", "--read-write-mode",
               *applied_expression(function, arguments)]
    result = subprocess.run(command, cwd=NIX_DIRECTORY, capture_output=True, text=True)
    if result.returncode:
        raise NixEvaluationError(result.stderr)
    return json.loads(result.stdout)


def nix_eval_string(function: str, arguments: "DataclassInstance") -> str:
    result = nix_eval(function, arguments)
    if not isinstance(result, str):
        raise TypeError(f"the Nix expression returned {type(result).__name__} where a string was expected")
    return result
