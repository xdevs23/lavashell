# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  pkgs = import input.nixpkgs { };
  nix = /. + input.nix;
  artifactLayers = import (nix + "/artifact-layers.nix") { inherit (pkgs) lib; };
  buildScope = import (nix + "/cargo-build-scope.nix") { inherit pkgs; };
  removeStubbedMembers = import (nix + "/stubbed-member-removal.nix") { inherit pkgs; };
  layer = {
    outPath = input.layer;
    previous = [ ];
  };
in
{
  buildLayer =
    artifactLayers.restoreLowerLayers [ ]
    + ''
      cargo build ${buildScope.target} --package ${pkgs.lib.escapeShellArg input.consumer}
    ''
    + removeStubbedMembers [
      "--package"
      input.stubbed
    ]
    + artifactLayers.storeLayerChanges;
  restore = artifactLayers.restore [ layer ];
}
