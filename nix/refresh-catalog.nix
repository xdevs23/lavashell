# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  checkout = import ./checkout-sources.nix { inherit (input) nixpkgs root; };
  inherit (checkout) sources directory;
  types = import ./types.nix;
  routeOf =
    relative:
    sources.layerRoute {
      package = sources.packageName (directory relative);
      crates = sources.localCrates (directory relative);
    };
in
{
  needsEngine = builtins.listToAttrs (
    map (relative: {
      name = relative;
      value = routeOf relative == types.layerRoutes.frameworkWithEngine;
    }) input.directories
  );
  inherit (sources) nativeNames;
}
