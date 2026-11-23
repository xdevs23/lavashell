# SPDX-License-Identifier: GPL-3.0-only
{ lib, buildLayers }:
let
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  artifactRecord = import ./cargo-artifact-record.nix;
in
lowerLayers:
let
  reusedStubLayers = lib.filter (layer: layer.stubLayer) (artifactLayers.stack lowerLayers);
  stubLayerFlags = lib.concatMap (layer: [
    "--stub-layer"
    "${layer}/${artifactRecord.installedPath}"
  ]) reusedStubLayers;
in
{
  inherit reusedStubLayers;
  script = ''
    ${lib.getExe' buildLayers "lavashell-plan-check"} stub-layer-reuse --build ${artifactRecord.buildPath} \
      ${lib.escapeShellArgs stubLayerFlags}
  '';
}
