# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  derivations = import ./layer-derivations.nix { inherit arguments; };
in
builtins.filter (key: !(builtins.tryEval derivations.${key}).success) (
  builtins.attrNames derivations
)
