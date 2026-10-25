#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check the architecture, page alignment, dependencies and API level of an AOSP shared library.

The API level is the newest level that introduced a library or symbol that the library imports. The NDK stub of an
API level contains the symbols of that level and of all older levels. The oldest stub that defines a symbol
therefore gives the API level that introduced the symbol.
"""

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path
import subprocess
import sys


@dataclass(frozen=True)
class Segment:
    kind: str
    offset: int
    address: int
    memory_size: int
    alignment: int


@dataclass(frozen=True)
class Introductions:
    """The oldest API level that defines each system library, by file name, and each symbol of those libraries."""

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


def symbol_name(symbol: dict) -> str:
    """The name of a dynamic symbol without the version that llvm-readobj appends after an at sign."""
    return symbol["Name"]["Name"].partition("@")[0]


def decode_library(document: str) -> Library:
    entries = json.loads(document)
    if not isinstance(entries, list) or len(entries) != 1:
        raise ValueError("llvm-readobj must report exactly one ELF object")
    elf = entries[0]
    segments = tuple(
        Segment(
            kind=header["Type"]["Name"],
            offset=header["Offset"],
            address=header["VirtualAddress"],
            memory_size=header["MemSize"],
            alignment=header["Alignment"],
        )
        for entry in elf["ProgramHeaders"]
        for header in [entry["ProgramHeader"]]
    )
    symbols = [entry["Symbol"] for entry in elf["DynamicSymbols"]]
    exports = frozenset(
        symbol_name(symbol)
        for symbol in symbols
        if symbol["Section"]["Value"] != 0
        and symbol["Binding"]["Name"] in {"Global", "Weak"}
        and symbol["Other"]["Value"] & 3 in {0, 3}
    )
    imports = frozenset(
        symbol_name(symbol)
        for symbol in symbols
        if symbol["Section"]["Value"] == 0 and symbol["Binding"]["Name"] == "Global" and symbol_name(symbol)
    )
    return Library(
        arch=elf["FileSummary"]["Arch"],
        kind=elf["ElfHeader"]["Type"],
        segments=segments,
        needed=tuple(elf["NeededLibraries"]),
        exports=exports,
        imports=imports,
    )


def read_library(llvm: Path, path: Path) -> Library:
    command = [
        str(llvm / "llvm-readobj"), "--elf-output-style=JSON", "--file-headers",
        "--program-headers", "--needed-libs", "--dyn-symbols", str(path),
    ]
    return decode_library(subprocess.run(command, check=True, capture_output=True, text=True).stdout)


def levels_of(system_libraries: Path) -> list[int]:
    """The API levels that `system_libraries` has stubs for, oldest first."""
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


def verify(library: Library, arch: str, page_size: int, introduced: Introductions, api_level: int) -> dict:
    failures = []
    if library.arch != arch:
        failures.append(f"ELF architecture is {library.arch}, expected {arch}")
    if library.kind != "SharedObject (0x3)":
        failures.append(f"ELF type is {library.kind}, expected a shared object")
    loads = [segment for segment in library.segments if segment.kind == "PT_LOAD"]
    if not loads:
        failures.append("ELF has no PT_LOAD segments")
    for index, segment in enumerate(loads):
        alignment = segment.alignment
        if alignment < page_size or alignment & (alignment - 1):
            failures.append(f"PT_LOAD[{index}] alignment {alignment} must be a power of two of at least {page_size}")
        elif (segment.address - segment.offset) % alignment:
            failures.append(f"PT_LOAD[{index}] virtual address and file offset differ modulo alignment {alignment}")
    relro = [segment for segment in library.segments if segment.kind == "PT_GNU_RELRO"]
    if not relro:
        failures.append("ELF has no PT_GNU_RELRO segment")
    relro_ends = [segment.address + segment.memory_size for segment in relro]
    for index, end in enumerate(relro_ends):
        if end % page_size:
            failures.append(f"PT_GNU_RELRO[{index}] end {end:#x} is not aligned to {page_size} bytes")
    missing = [
        name for name in library.needed
        if Path(name).name != name or name not in introduced.libraries
    ]
    if missing:
        failures.append(f"DT_NEEDED libraries have no system stubs: {', '.join(missing)}")
    unresolved = sorted(library.imports - introduced.symbols.keys())
    if unresolved:
        failures.append(f"imported symbols that no system library defines: {', '.join(unresolved)}")
    required_level = max(
        [introduced.libraries[name] for name in library.needed if name in introduced.libraries]
        + [introduced.symbols[name] for name in library.imports if name in introduced.symbols],
        default=None,
    )
    if required_level != api_level:
        failures.append(f"the API level is {api_level}, but the newest import was introduced in {required_level}")
    return {
        "status": "FAIL" if failures else "PASS",
        "arch": library.arch,
        "expected_arch": arch,
        "elf_type": library.kind,
        "page_size": page_size,
        "load_segments": [asdict(segment) for segment in loads],
        "relro_ends": relro_ends,
        "needed_libraries": library.needed,
        "missing_system_libraries": missing,
        "unresolved_symbols": unresolved,
        "api_level": api_level,
        "required_api_level": required_level,
        "failures": failures,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("library", type=Path)
    parser.add_argument("--llvm", type=Path, required=True, help="Directory containing llvm-readobj")
    parser.add_argument(
        "--system-libraries", type=Path, required=True,
        help="NDK directory of the system stubs of the ABI, with one subdirectory per API level",
    )
    parser.add_argument("--api-level", type=int, required=True, help="API level that the library was built for")
    parser.add_argument("--page-size", type=int, required=True, help="Page size in bytes that segments align to")
    parser.add_argument("--arch", required=True, help="Architecture as llvm-readobj names it, such as aarch64")
    args = parser.parse_args()
    report = {"library": str(args.library), "system_libraries": str(args.system_libraries)}
    try:
        if not args.system_libraries.is_dir():
            raise ValueError(f"system library directory is unavailable: {args.system_libraries}")
        with args.library.open("rb") as source:
            report["sha256"] = hashlib.file_digest(source, "sha256").hexdigest()
        library = read_library(args.llvm, args.library)
        introduced = introductions(args.llvm, args.system_libraries, library.needed)
        report.update(verify(library, args.arch, args.page_size, introduced, args.api_level))
    except subprocess.CalledProcessError as error:
        report.update(status="ERROR", failures=[f"llvm-readobj exited {error.returncode}: {error.stderr.strip()}"])
    except (OSError, ValueError, KeyError, TypeError) as error:
        report.update(status="ERROR", failures=[f"cannot inspect AOSP library: {error}"])
    print(json.dumps(report, indent=2))
    return {"PASS": 0, "FAIL": 1, "ERROR": 2}[report["status"]]


if __name__ == "__main__":
    sys.exit(main())
