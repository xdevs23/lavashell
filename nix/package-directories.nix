# SPDX-License-Identifier: GPL-3.0-only
{ lib, root }:
let
  contract = import ./projection-contract.nix;
  runnable = lib.filterAttrs (
    name: type:
    type == "directory"
    && builtins.pathExists (root + "/examples/${name}/Cargo.toml")
    && builtins.pathExists (root + "/examples/${name}/src/main.rs")
  ) (builtins.readDir (root + "/examples"));
  examples = lib.mapAttrs (name: _: root + "/examples/${name}") runnable;
  packaged = lib.mapAttrs (_: directory: root + "/${directory}") contract.packagedCrates;
in
{
  inherit examples packaged;
  all = lib.attrsets.unionOfDisjoint examples packaged;
}
