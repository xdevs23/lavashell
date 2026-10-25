# SPDX-License-Identifier: GPL-3.0-only
# Removes each patch of a crate that is absent from the lock file. Cargo would record such a patch as unused in the
# lock file, which a frozen build cannot update.
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
