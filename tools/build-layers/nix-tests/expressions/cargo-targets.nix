# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
in
import (/. + input.nix + "/cargo-targets.nix") { lib = import (input.nixpkgs + "/lib"); } {
  key = input.directory;
  inherit (input) manifest;
}
