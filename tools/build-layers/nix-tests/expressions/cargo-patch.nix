# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
in
import (/. + input.nix + "/cargo-patch.nix") {
  lib = import (input.nixpkgs + "/lib");
} input.patch input.lock
