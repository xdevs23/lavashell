#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import enum
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import Any

from next_steps import NextStep

SHARED_OBJECT = "SharedObject (0x3)"
# https://refspecs.linuxfoundation.org/elf/gabi4+/ch4.symtab.html
VISIBILITY_MASK = 0b11
DEFAULT_VISIBILITY = 0
PROTECTED_VISIBILITY = 3


class Status(enum.Enum):
    PASS = 0
    FAIL = 1
    ERROR = 2

    @property
    def exit_code(self) -> int:
        return self.value

    @property
    def next_step(self) -> NextStep | None:
        match self:
            case Status.PASS:
                return None
            case Status.FAIL:
                return NextStep.FIX_LISTED_PROBLEMS
            case Status.ERROR:
                return NextStep.FIX_CHECK_ERROR


class SegmentKind(enum.Enum):
    LOAD = enum.auto()
    RELRO = enum.auto()
    OTHER = enum.auto()


class Binding(enum.Enum):
    GLOBAL = enum.auto()
    WEAK = enum.auto()
    OTHER = enum.auto()


SEGMENT_KINDS = {"PT_LOAD": SegmentKind.LOAD, "PT_GNU_RELRO": SegmentKind.RELRO}
BINDINGS = {"Global": Binding.GLOBAL, "Weak": Binding.WEAK}


@dataclass(frozen=True)
class Segment:
    kind: SegmentKind
    offset: int
    address: int
    memory_size: int
    alignment: int


@dataclass(frozen=True)
class Symbol:
    name: str
    defined: bool
    binding: Binding
    visibility: int

    @property
    def exported(self) -> bool:
        return (
            self.defined
            and self.binding in {Binding.GLOBAL, Binding.WEAK}
            and self.visibility in {DEFAULT_VISIBILITY, PROTECTED_VISIBILITY}
        )

    @property
    def imported(self) -> bool:
        return not self.defined and self.binding is Binding.GLOBAL and bool(self.name)


@dataclass(frozen=True)
class Introductions:
    libraries: dict[str, int]
    symbols: dict[str, int]


@dataclass(frozen=True)
class Library:
    arch: str
    kind: str
    segments: tuple[Segment, ...]
    needed: tuple[str, ...]
    exports: frozenset[str]
    imports: frozenset[str]


@dataclass(frozen=True)
class Verdict:
    arch: str
    expected_arch: str
    elf_type: str
    page_size: int
    load_segments: tuple[Segment, ...]
    relro_ends: tuple[int, ...]
    needed_libraries: tuple[str, ...]
    missing_system_libraries: tuple[str, ...]
    unresolved_symbols: tuple[str, ...]
    api_level: int
    required_api_level: int | None
    failures: tuple[str, ...]

    @property
    def status(self) -> Status:
        return Status.FAIL if self.failures else Status.PASS


@dataclass(frozen=True)
class Settings:
    library: Path
    llvm: Path
    system_libraries: Path
    api_level: int
    page_size: int
    arch: str

    @classmethod
    def read(cls) -> Settings:
        parser = argparse.ArgumentParser()
        parser.add_argument("library", type=Path)
        parser.add_argument("--llvm", type=Path, required=True, help="Directory containing llvm-readobj")
        parser.add_argument(
            "--system-libraries", type=Path, required=True,
            help="NDK directory of the system stubs of the ABI, with one subdirectory per API level",
        )
        parser.add_argument("--api-level", type=int, required=True, help="API level that the library was built for")
        parser.add_argument("--page-size", type=int, required=True, help="Page size in bytes that segments align to")
        parser.add_argument("--arch", required=True, help="Architecture as llvm-readobj names it, such as aarch64")
        return cls(**vars(parser.parse_args()))


@dataclass(frozen=True)
class Report:
    library: str
    system_libraries: str
    sha256: str | None
    status: Status
    verdict: Verdict | None
    error: str | None
    next_step: NextStep | None


def decode_segment(header: Any) -> Segment:
    return Segment(
        kind=SEGMENT_KINDS.get(header["Type"]["Name"], SegmentKind.OTHER),
        offset=header["Offset"],
        address=header["VirtualAddress"],
        memory_size=header["MemSize"],
        alignment=header["Alignment"],
    )


def decode_symbol(symbol: Any) -> Symbol:
    return Symbol(
        name=symbol["Name"]["Name"].partition("@")[0],
        defined=symbol["Section"]["Value"] != 0,
        binding=BINDINGS.get(symbol["Binding"]["Name"], Binding.OTHER),
        visibility=symbol["Other"]["Value"] & VISIBILITY_MASK,
    )


def decode_library(document: str) -> Library:
    entries = json.loads(document)
    if not isinstance(entries, list) or len(entries) != 1:
        raise ValueError("llvm-readobj must report exactly one ELF object")
    elf = entries[0]
    try:
        symbols = [decode_symbol(entry["Symbol"]) for entry in elf["DynamicSymbols"]]
        return Library(
            arch=elf["FileSummary"]["Arch"],
            kind=elf["ElfHeader"]["Type"],
            segments=tuple(decode_segment(entry["ProgramHeader"]) for entry in elf["ProgramHeaders"]),
            needed=tuple(elf["NeededLibraries"]),
            exports=frozenset(symbol.name for symbol in symbols if symbol.exported),
            imports=frozenset(symbol.name for symbol in symbols if symbol.imported),
        )
    except KeyError as missing:
        raise ValueError(f"llvm-readobj reports no {missing}") from missing


def read_library(llvm: Path, path: Path) -> Library:
    command = [
        str(llvm / "llvm-readobj"), "--elf-output-style=JSON", "--file-headers",
        "--program-headers", "--needed-libs", "--dyn-symbols", str(path),
    ]
    return decode_library(subprocess.run(command, check=True, capture_output=True, text=True).stdout)


def levels_of(system_libraries: Path) -> list[int]:
    return sorted(int(entry.name) for entry in system_libraries.iterdir() if entry.name.isdecimal())


def introductions(llvm: Path, system_libraries: Path, needed: tuple[str, ...]) -> Introductions:
    found = Introductions(libraries={}, symbols={})
    for level in levels_of(system_libraries):
        for name in needed:
            stub = system_libraries / str(level) / name
            if stub.is_file():
                found.libraries.setdefault(name, level)
                for symbol in read_library(llvm, stub).exports:
                    found.symbols.setdefault(symbol, level)
    return found


def architecture_failures(library: Library, arch: str) -> list[str]:
    if library.arch != arch:
        return [f"ELF architecture is {library.arch}, expected {arch}"]
    return []


def type_failures(library: Library) -> list[str]:
    if library.kind != SHARED_OBJECT:
        return [f"ELF type is {library.kind}, expected a shared object"]
    return []


def load_failures(loads: list[Segment], page_size: int) -> list[str]:
    if not loads:
        return ["ELF has no PT_LOAD segments"]
    failures = []
    for index, segment in enumerate(loads):
        alignment = segment.alignment
        if alignment < page_size or alignment & (alignment - 1):
            failures.append(f"PT_LOAD[{index}] alignment {alignment} must be a power of two of at least {page_size}")
        elif (segment.address - segment.offset) % alignment:
            failures.append(f"PT_LOAD[{index}] virtual address and file offset differ modulo alignment {alignment}")
    return failures


def relro_failures(relro_ends: list[int], page_size: int) -> list[str]:
    if not relro_ends:
        return ["ELF has no PT_GNU_RELRO segment"]
    return [
        f"PT_GNU_RELRO[{index}] end {end:#x} is not aligned to {page_size} bytes"
        for index, end in enumerate(relro_ends)
        if end % page_size
    ]


def missing_system_libraries(library: Library, introduced: Introductions) -> list[str]:
    return [name for name in library.needed if Path(name).name != name or name not in introduced.libraries]


def unresolved_symbols(library: Library, introduced: Introductions) -> list[str]:
    return sorted(library.imports - introduced.symbols.keys())


def required_api_level(library: Library, introduced: Introductions) -> int | None:
    return max(
        [introduced.libraries[name] for name in library.needed if name in introduced.libraries]
        + [introduced.symbols[name] for name in library.imports if name in introduced.symbols],
        default=None,
    )


def verify(library: Library, arch: str, page_size: int, introduced: Introductions, api_level: int) -> Verdict:
    loads = [segment for segment in library.segments if segment.kind is SegmentKind.LOAD]
    relro = [segment for segment in library.segments if segment.kind is SegmentKind.RELRO]
    relro_ends = [segment.address + segment.memory_size for segment in relro]
    missing = missing_system_libraries(library, introduced)
    unresolved = unresolved_symbols(library, introduced)
    required_level = required_api_level(library, introduced)
    failures = architecture_failures(library, arch) + type_failures(library)
    failures += load_failures(loads, page_size) + relro_failures(relro_ends, page_size)
    if missing:
        failures.append(f"DT_NEEDED libraries have no system stubs: {', '.join(missing)}")
    if unresolved:
        failures.append(f"imported symbols that no system library defines: {', '.join(unresolved)}")
    if required_level != api_level:
        failures.append(f"the API level is {api_level}, but the newest import was introduced in {required_level}")
    return Verdict(
        arch=library.arch,
        expected_arch=arch,
        elf_type=library.kind,
        page_size=page_size,
        load_segments=tuple(loads),
        relro_ends=tuple(relro_ends),
        needed_libraries=library.needed,
        missing_system_libraries=tuple(missing),
        unresolved_symbols=tuple(unresolved),
        api_level=api_level,
        required_api_level=required_level,
        failures=tuple(failures),
    )


def json_text(value: enum.Enum) -> str:
    return value.text if isinstance(value, NextStep) else value.name


def report_text(report: Report) -> str:
    return json.dumps(asdict(report), indent=2, default=json_text)


def inspect_library(settings: Settings) -> Report:
    digest: str | None = None
    try:
        if not settings.system_libraries.is_dir():
            raise ValueError(f"system library directory is unavailable: {settings.system_libraries}")
        with settings.library.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        library = read_library(settings.llvm, settings.library)
        introduced = introductions(settings.llvm, settings.system_libraries, library.needed)
        verdict = verify(library, settings.arch, settings.page_size, introduced, settings.api_level)
        status, error = verdict.status, None
    except subprocess.CalledProcessError as failure:
        verdict, status = None, Status.ERROR
        error = f"llvm-readobj exited {failure.returncode}: {failure.stderr.strip()}"
    except (OSError, ValueError) as failure:
        verdict, status = None, Status.ERROR
        error = f"cannot inspect AOSP library: {failure}"
    return Report(
        library=str(settings.library),
        system_libraries=str(settings.system_libraries),
        sha256=digest,
        status=status,
        verdict=verdict,
        error=error,
        next_step=status.next_step,
    )


def main() -> int:
    report = inspect_library(Settings.read())
    print(report_text(report))
    return report.status.exit_code


if __name__ == "__main__":
    sys.exit(main())
