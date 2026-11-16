# SPDX-License-Identifier: GPL-3.0-only

import subprocess
from collections.abc import Mapping
from pathlib import Path

from verification_errors import CommandFailed, VerificationError


def run_checked(command: list[str], failure: type[CommandFailed], cwd: Path | None = None,
                env: Mapping[str, str] | None = None) -> str:
    result = subprocess.run(command, capture_output=True, text=True, cwd=cwd, env=env)
    if result.returncode:
        raise VerificationError(failure(command=tuple(command), stderr=result.stderr))
    return result.stdout
