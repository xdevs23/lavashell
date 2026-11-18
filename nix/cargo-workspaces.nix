# SPDX-License-Identifier: GPL-3.0-only
{ lib, root }:
let
  directories = {
    main = ".";
    buildLayers = "tools/build-layers";
  };
  listed = map (directory: root + "/${directory}") (lib.attrValues directories);
  inTree = map dirOf (
    lib.filter (file: baseNameOf file == "Cargo.toml" && lib.importTOML file ? workspace) (
      lib.filesystem.listFilesRecursive root
    )
  );
  unlisted = lib.subtractLists listed inTree;
in
assert lib.assertMsg (
  unlisted == [ ]
) "nix/cargo-workspaces.nix does not list the Cargo workspaces ${toString unlisted}";
directories
