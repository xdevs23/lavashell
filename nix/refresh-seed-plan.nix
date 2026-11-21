# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  checkout = import ./checkout-sources.nix { inherit (input) nixpkgs root projections; };
in
checkout.sources.seedPlan (checkout.directory input.relative) input.layer
