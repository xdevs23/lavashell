# SPDX-License-Identifier: GPL-3.0-only
{ pkgs, buildLayers }:
let
  inherit (pkgs) lib;
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  artifactRecord = import ./cargo-artifact-record.nix { inherit pkgs; };
in
{ checkedBuild, lowerLayers }:
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
    ${lib.getExe' buildLayers "lavashell-plan-check"} --build ${artifactRecord.buildPath} \
      --checked-build ${checkedBuild} ${lib.escapeShellArgs stubLayerFlags}
  '';
}
