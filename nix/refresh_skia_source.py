#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import ast
import json
from collections.abc import Iterable
from dataclasses import asdict, dataclass
from functools import partial
from pathlib import Path

from cargo_files import read_toml
from checked_commands import run_checked
from concurrent_checks import run_concurrently
from repository_paths import REPOSITORY_ROOT
from document_tables import DocumentTable
from verification_errors import FetchFailed, SkiaDepsUnreadable, SkiaExternalUnreadable, VerificationError

LICENSES_FILE = Path(__file__).with_name("skia-licenses.json")
SOURCE_FILE = Path(__file__).with_name("skia-source.json")
EXTERNALS_DIRECTORY = "third_party/externals/"
OWNER = "rust-skia"
REPOSITORY = "skia"


@dataclass(frozen=True)
class Fork:
    git: str
    rev: str

    @classmethod
    def read(cls, manifest: Path) -> "Fork":
        patch = read_toml(manifest).object("patch").object("crates-io").object("skia-bindings")
        return cls(git=patch.string("git"), rev=patch.string("rev"))


@dataclass(frozen=True)
class Fetched:
    hash: str
    path: Path


@dataclass(frozen=True)
class Pin:
    url: str
    rev: str


@dataclass(frozen=True)
class External:
    url: str
    rev: str
    hash: str


@dataclass(frozen=True)
class SkiaSource:
    owner: str
    repo: str
    fork_rev: str
    tag: str
    hash: str
    externals: dict[str, External]


def prefetch_git(url: str, rev: str) -> Fetched:
    fetched = DocumentTable.parse(run_checked(["nix-prefetch-git", "--quiet", "--url", url, "--rev", rev], FetchFailed),
                                  "nix-prefetch-git")
    return Fetched(hash=fetched.string("hash"), path=Path(fetched.string("path")))


def prefetch_archive(url: str) -> Fetched:
    command = ["nix", "store", "prefetch-file", "--unpack", "--json", url]
    fetched = DocumentTable.parse(run_checked(command, FetchFailed), "nix store prefetch-file")
    return Fetched(hash=fetched.string("hash"), path=Path(fetched.string("storePath")))


def pinned_externals(deps_source: str, names: Iterable[str]) -> dict[str, Pin]:
    assignments = (node for node in ast.parse(deps_source).body if isinstance(node, ast.Assign))
    table = next((node.value for node in assignments
                  if any(isinstance(target, ast.Name) and target.id == "deps" for target in node.targets)), None)
    if not isinstance(table, ast.Dict):
        raise VerificationError(SkiaDepsUnreadable())
    declared = {key.value: value for key, value in zip(table.keys, table.values, strict=True)
                if isinstance(key, ast.Constant)}
    pins: dict[str, Pin] = {}
    for name in names:
        value = declared.get(EXTERNALS_DIRECTORY + name)
        if not (isinstance(value, ast.Constant) and isinstance(value.value, str) and "@" in value.value):
            raise VerificationError(SkiaExternalUnreadable(name=name))
        url, _, rev = value.value.rpartition("@")
        pins[name] = Pin(url, rev)
    return pins


def fetch_external(name: str, pin: Pin) -> tuple[str, External]:
    return name, External(pin.url, pin.rev, prefetch_git(pin.url, pin.rev).hash)


def refresh() -> None:
    fork = Fork.read(REPOSITORY_ROOT / "Cargo.toml")
    bindings = read_toml(prefetch_git(fork.git, fork.rev).path / "skia-bindings/Cargo.toml")
    tag = bindings.object("package").object("metadata").string("skia")
    archive = prefetch_archive(f"https://github.com/{OWNER}/{REPOSITORY}/archive/refs/tags/{tag}.tar.gz")
    names = DocumentTable.parse(LICENSES_FILE.read_text(), LICENSES_FILE.name).keys()
    pins = pinned_externals((archive.path / "DEPS").read_text(), names)
    externals = dict(run_concurrently([partial(fetch_external, name, pin) for name, pin in pins.items()]))
    source = SkiaSource(OWNER, REPOSITORY, fork.rev, tag, archive.hash, externals)
    SOURCE_FILE.write_text(json.dumps(asdict(source), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    refresh()
