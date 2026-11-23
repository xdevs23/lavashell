# SPDX-License-Identifier: GPL-3.0-only
{ lib, buildLayers }:
let
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
in
lowerLayers:
let
  reusedStubLayers = lib.filter (layer: layer.stubLayer) (artifactLayers.stack lowerLayers);
  stubLayerFlags = lib.concatMap (layer: [
    "--stub-layer"
    "${layer}/${artifactLayers.buildRecord}"
  ]) reusedStubLayers;
in
{
  inherit reusedStubLayers;
  script = ''
    ${lib.getExe' buildLayers "lavashell-plan-check"} stub-layer-reuse --build cargo-artifacts.jsonl \
      ${lib.escapeShellArgs stubLayerFlags}
  '';
}
