#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check that the Cargo plan of the consumer contains every external build unit compiled in the engine layer, and
that the engine layer contains every Bevy unit of the consumer. The check runs before the local framework code
compiles, so a stale projection fails the build early."""

import argparse
import importlib.util
import json
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--engine", required=True, type=Path)
    parser.add_argument("--consumer", required=True, type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("cargo_plan", args.library)
    plan = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plan)
    metadata = json.loads(args.engine.read_text())
    consumer = json.loads(args.consumer.read_text())
    plan.verify(metadata["externalUnits"], consumer)
    print(f"Verified {len(metadata['externalUnits'])} compatible engine external units")


if __name__ == "__main__":
    main()
