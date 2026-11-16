# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
in
import (/. + input.nix + "/cache-copy.nix") { lib = import (input.nixpkgs + "/lib"); } input.caches
