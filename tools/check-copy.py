#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
import csv
from pathlib import Path

root = Path(__file__).resolve().parents[1]
catalogs = sorted((root / "examples").glob("*/copy/*.intents.tsv"))
assert catalogs, "no intent catalogs found"
for catalog in catalogs:
    area = catalog.name.removesuffix(".intents.tsv")
    with catalog.open() as source:
        intents = {row["key"]: row for row in csv.DictReader(source, delimiter="\t")}
    with catalog.with_name(f"{area}.en.tsv").open() as source:
        rows = list(csv.reader(source, delimiter="\t"))
    assert all(len(row) == 2 for row in rows), "every copy row needs a key and value"
    copy = dict(rows)
    assert len(copy) == len(rows), "copy keys must be unique"
    assert copy.keys() == intents.keys(), "copy and intent keys must match"
    for key, value in copy.items():
        assert value.strip(), f"empty copy: {area}/{key}"
        assert not any(
            mark in value for mark in [";", "—", "–", "\n", "\r"]
        ), f"unsupported punctuation: {area}/{key}"
    print(f"Checked {area}: {len(copy)} copy keys.")
