# SPDX-License-Identifier: GPL-3.0-only
"""Enforce the crate rules that neither the compiler nor clippy check.

Every dependency of the workspace is an exact `=` version or a path, and every member is listed there as a path.
Every member inherits its dependencies, version, edition and lints from the workspace.
"""

from pathlib import Path
import sys
import tomllib

DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def is_pinned(entry):
    """Whether a workspace dependency specifies an exact version or a path."""
    if isinstance(entry, str):
        return entry.startswith("=")
    return "path" in entry or entry.get("version", "").startswith("=")


def inherits(entry):
    return isinstance(entry, dict) and entry.get("workspace") is True


def dependency_tables(manifest):
    tables = [manifest]
    tables.extend(manifest.get("target", {}).values())
    for table in tables:
        for name in DEPENDENCY_TABLES:
            yield from table.get(name, {}).items()


def manifest_failures(manifest, name):
    """The workspace rules that a member manifest breaks."""
    failures = [f"{name}: dependency {dependency} is not taken from the workspace"
                for dependency, entry in dependency_tables(manifest) if not inherits(entry)]
    failures.extend(f"{name}: package.{key} is not taken from the workspace"
                    for key in ("version", "edition") if not inherits(manifest["package"].get(key)))
    if manifest.get("lints") != {"workspace": True}:
        failures.append(f"{name}: lints are not taken from the workspace")
    return failures


def unlisted_members(workspace):
    """The members that the workspace dependencies do not list as a path."""
    entries = workspace["dependencies"].values()
    paths = {entry["path"] for entry in entries if isinstance(entry, dict) and "path" in entry}
    return [member for member in workspace["members"] if member not in paths]


def failures_of(root):
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    failures = [f"Cargo.toml: workspace dependency {name} is not an exact version or a path"
                for name, entry in workspace["dependencies"].items() if not is_pinned(entry)]
    failures.extend(f"Cargo.toml: member {member} is not a path entry of the workspace dependencies"
                    for member in unlisted_members(workspace))
    for member in workspace["members"]:
        manifest = tomllib.loads((root / member / "Cargo.toml").read_text())
        failures.extend(manifest_failures(manifest, member))
    return failures


if __name__ == "__main__":
    problems = failures_of(Path(__file__).resolve().parents[1])
    print("\n".join(problems) if problems else "Passed crate rules")
    sys.exit(bool(problems))
