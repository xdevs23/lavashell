# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  checkout = import ./checkout-sources.nix { inherit (input) nixpkgs root projections; };
  inherit (checkout) sources directory;
  types = import ./types.nix;
in
{
  needsEngine = builtins.listToAttrs (
    map (relative: {
      name = relative;
      value = sources.layerRouteOf (directory relative) == types.layerRoutes.frameworkWithEngine;
    }) input.directories
  );
  inherit (sources) nativeNames;
}
