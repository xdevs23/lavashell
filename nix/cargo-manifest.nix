# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
rec {
  dependencyKinds = [
    "dependencies"
    "dev-dependencies"
    "build-dependencies"
  ];
  dependencyTables =
    manifest:
    map (kind: manifest.${kind} or { }) dependencyKinds
    ++ lib.concatMap dependencyTables (builtins.attrValues (manifest.target or { }));
}
