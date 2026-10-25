# SPDX-License-Identifier: GPL-3.0-only
"""Check that every source file and document in the workspace has fewer than 500 lines and lines of at most 120
characters.

The width of Python lines is checked by ruff, which the source check of the flake runs next to this script.
"""

from pathlib import Path
import sys

root = Path(__file__).resolve().parents[1]
suffixes = {".rs", ".py", ".kt", ".sksl", ".nix", ".sh", ".md"}
failures = []
count = 0
paths = [*root.glob("*.nix"), *root.glob("*.md")]
for folder in ("crates", "docs", "examples", "tools", "nix"):
    paths.extend((root / folder).rglob("*"))
for path in paths:
    if path.suffix not in suffixes or "node_modules" in path.parts:
        continue
    count += 1
    lines = path.read_text().splitlines()
    relative = path.relative_to(root)
    if len(lines) >= 500:
        failures.append(f"{relative}: {len(lines)} lines (must be below 500)")
    if path.suffix == ".py":
        continue
    for number, line in enumerate(lines, 1):
        if len(line) > 120:
            failures.append(f"{relative}:{number}: {len(line)} columns (maximum 120)")
print("\n".join(failures) if failures else f"Passed source limits for {count} files")
sys.exit(bool(failures))
