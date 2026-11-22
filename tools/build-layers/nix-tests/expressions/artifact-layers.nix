# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  lib = import (input.nixpkgs + "/lib");
  artifactLayers = import (/. + input.nix + "/artifact-layers.nix") { inherit lib; };
  layers = lib.mapAttrs (name: layer: {
    inherit name;
    outPath = layer.output;
    previous = map (lower: layers.${lower}) layer.previous;
  }) input.layers;
  restored = map (name: layers.${name}) input.restored;
in
{
  stack = map (layer: layer.name) (artifactLayers.stack restored);
  restore = artifactLayers.restore restored;
  restoreLowerLayers = artifactLayers.restoreLowerLayers restored;
  inherit (artifactLayers) storeLayerChanges;
}
