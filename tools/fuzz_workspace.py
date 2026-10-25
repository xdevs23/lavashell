# SPDX-License-Identifier: GPL-3.0-only
"""Check that the fuzz workspace has the edition, the shared dependency versions and the lints of
the root workspace."""

from dataclasses import dataclass
from pathlib import Path
import sys
import tomllib

type Dependency = str | dict[str, str | bool | list[str]]
type Lints = dict[str, dict[str, str | dict[str, str | int]]]

FUZZ_MANIFEST = Path("crates/lavashell-protocol/fuzz/Cargo.toml")


@dataclass(frozen=True)
class Workspace:
    """The values that the fuzz workspace copies from the root workspace."""

    edition: str
    versions: dict[str, str | None]
    lints: Lints


def versions(dependencies: dict[str, Dependency]) -> dict[str, str | None]:
    """The version required for each dependency, and `None` for a dependency given by path."""
    return {
        name: spec if isinstance(spec, str) else spec.get("version")
        for name, spec in dependencies.items()
    }


def root_workspace(root: Path) -> Workspace:
    with open(root / "Cargo.toml", "rb") as manifest:
        workspace = tomllib.load(manifest)["workspace"]
    return Workspace(
        workspace["package"]["edition"], versions(workspace["dependencies"]), workspace["lints"]
    )


def fuzz_workspace(root: Path) -> Workspace:
    with open(root / FUZZ_MANIFEST, "rb") as manifest:
        fuzz = tomllib.load(manifest)
    workspace = fuzz["workspace"]
    return Workspace(
        fuzz["package"]["edition"], versions(workspace["dependencies"]), workspace["lints"]
    )


def differences(root: Workspace, fuzz: Workspace) -> list[str]:
    """What the fuzz workspace sets apart from the root workspace."""
    found = []
    if fuzz.edition != root.edition:
        found.append(f"edition {fuzz.edition}, the root workspace has {root.edition}")
    found += [
        f"{name} {version}, the root workspace has {root.versions[name]}"
        for name, version in fuzz.versions.items()
        if name in root.versions and version != root.versions[name]
    ]
    if fuzz.lints != root.lints:
        found.append("lints that differ from those of the root workspace")
    return found


if __name__ == "__main__":
    checkout = Path(__file__).resolve().parents[1]
    problems = differences(root_workspace(checkout), fuzz_workspace(checkout))
    print("\n".join(f"fuzz workspace: {problem}" for problem in problems)
          or "The fuzz workspace matches the root workspace")
    sys.exit(bool(problems))
