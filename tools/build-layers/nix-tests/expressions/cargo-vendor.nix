# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
in
(import (/. + input.nix + "/cargo-vendor.nix") {
  pkgs = import input.nixpkgs { };
  inherit (input) commits;
} (/. + input.lock)).drvPath
