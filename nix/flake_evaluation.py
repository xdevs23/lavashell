# SPDX-License-Identifier: GPL-3.0-only

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def evaluate(body, output="--raw"):
    """Evaluate `body`, which can refer to `flake` and `pkgs`, and return the finished `nix eval` process."""
    expression = f"""
      let
        flake = builtins.getFlake "git+file://{ROOT}";
        pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
      in {body}"""
    return subprocess.run(["nix", "eval", "--offline", "--impure", output, "--expr", expression],
                          capture_output=True, text=True)


def locked_nixpkgs():
    process = evaluate("flake.inputs.nixpkgs.outPath")
    if process.returncode:
        raise SystemExit(process.stderr)
    return process.stdout
