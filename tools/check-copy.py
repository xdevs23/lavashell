#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
import csv
from pathlib import Path

from next_steps import exit_with_check_result
from repository_root import REPOSITORY_ROOT

UNSUPPORTED_PUNCTUATION = (";", "—", "–", "\n", "\r")


def read_copy_rows(path: Path) -> list[list[str]]:
    with path.open() as source:
        return list(csv.reader(source, delimiter="\t", quoting=csv.QUOTE_NONE))


def catalog_failures(catalog: Path) -> list[str]:
    area = catalog.name.removesuffix(".en.tsv")
    rows = read_copy_rows(catalog)
    if any(len(row) != 2 for row in rows):
        return [f"{area}: every copy row needs a key and value"]
    copy = dict(rows)
    failures = []
    if len(copy) != len(rows):
        failures.append(f"{area}: copy keys must be unique")
    for key, value in copy.items():
        if not value.strip():
            failures.append(f"{area}/{key}: empty copy")
        if any(mark in value for mark in UNSUPPORTED_PUNCTUATION):
            failures.append(f"{area}/{key}: unsupported punctuation")
    return failures


def failures_of(root: Path) -> list[str]:
    catalogs = sorted((root / "examples").glob("*/copy/*.en.tsv"))
    if not catalogs:
        return ["no copy catalogs found"]
    return [failure for catalog in catalogs for failure in catalog_failures(catalog)]


if __name__ == "__main__":
    exit_with_check_result(failures_of(REPOSITORY_ROOT), "Passed copy checks")
