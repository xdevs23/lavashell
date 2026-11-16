# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
patch: lockData:
lib.filterAttrs (_: patches: patches != { }) (
  lib.mapAttrs (
    _:
    lib.filterAttrs (
      name: entry: lib.any (package: package.name == entry.package or name) lockData.package
    )
  ) patch
)
