# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  root = /. + input.root;
  workspaceCrates = import (/. + input.nix + "/workspace-crates.nix") {
    lib = import (input.nixpkgs + "/lib");
    inherit root;
  };
in
workspaceCrates.dependencyFeaturesEnabledByDependents {
  directory = root + "/${input.crate}";
  dependents = map (dependent: root + "/${dependent}") input.dependents;
}
