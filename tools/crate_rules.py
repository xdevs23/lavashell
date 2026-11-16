# SPDX-License-Identifier: GPL-3.0-only

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import tomllib
from typing import Any

from next_steps import exit_with_check_result
from repository_root import REPOSITORY_ROOT

DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def inherits_workspace(entry: Any) -> bool:
    return isinstance(entry, dict) and entry.get("workspace") is True


@dataclass(frozen=True)
class Dependency:
    name: str
    pinned: bool
    path: str | None
    inherited: bool

    @classmethod
    def read(cls, name: str, entry: str | dict[str, Any]) -> Dependency:
        if isinstance(entry, str):
            return cls(name=name, pinned=entry.startswith("="), path=None, inherited=False)
        path = entry.get("path")
        return cls(
            name=name,
            pinned=path is not None or entry.get("version", "").startswith("="),
            path=path,
            inherited=inherits_workspace(entry),
        )


@dataclass(frozen=True)
class Manifest:
    version_inherited: bool
    edition_inherited: bool
    lints_inherited: bool
    dependencies: tuple[Dependency, ...]

    @classmethod
    def read(cls, path: Path) -> Manifest:
        document = tomllib.loads(path.read_text())
        package = document["package"]
        tables = [document, *document.get("target", {}).values()]
        return cls(
            version_inherited=inherits_workspace(package.get("version")),
            edition_inherited=inherits_workspace(package.get("edition")),
            lints_inherited=document.get("lints") == {"workspace": True},
            dependencies=tuple(
                Dependency.read(name, entry)
                for table in tables
                for kind in DEPENDENCY_TABLES
                for name, entry in table.get(kind, {}).items()
            ),
        )


@dataclass(frozen=True)
class Workspace:
    members: tuple[str, ...]
    dependencies: tuple[Dependency, ...]

    @classmethod
    def read(cls, path: Path) -> Workspace:
        workspace = tomllib.loads(path.read_text())["workspace"]
        return cls(
            members=tuple(workspace["members"]),
            dependencies=tuple(Dependency.read(name, entry) for name, entry in workspace["dependencies"].items()),
        )


def manifest_failures(manifest: Manifest, name: str) -> list[str]:
    failures = [
        f"{name}: dependency {dependency.name} is not taken from the workspace"
        for dependency in manifest.dependencies
        if not dependency.inherited
    ]
    if not manifest.version_inherited:
        failures.append(f"{name}: package.version is not taken from the workspace")
    if not manifest.edition_inherited:
        failures.append(f"{name}: package.edition is not taken from the workspace")
    if not manifest.lints_inherited:
        failures.append(f"{name}: lints are not taken from the workspace")
    return failures


def failures_of(root: Path) -> list[str]:
    workspace = Workspace.read(root / "Cargo.toml")
    listed_paths = {dependency.path for dependency in workspace.dependencies}
    failures = [
        f"Cargo.toml: workspace dependency {dependency.name} is not an exact version or a path"
        for dependency in workspace.dependencies
        if not dependency.pinned
    ]
    failures.extend(
        f"Cargo.toml: member {member} is not a path entry of the workspace dependencies"
        for member in workspace.members
        if member not in listed_paths
    )
    for member in workspace.members:
        failures.extend(manifest_failures(Manifest.read(root / member / "Cargo.toml"), member))
    return failures


if __name__ == "__main__":
    exit_with_check_result(failures_of(REPOSITORY_ROOT), "Passed crate rules")
