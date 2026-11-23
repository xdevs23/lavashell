# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  pkgs = import input.nixpkgs { };
  root = /. + input.root;
  inherit
    ((import (/. + input.nix + "/examples-tree.nix") (
      import (/. + input.nix + "/sources-and-assets.nix") { inherit pkgs root; }
      // {
        inherit pkgs root;
        inherit (pkgs) rustPlatform;
      }
    )).examples
    )
    reference
    ;
  derivationsOf = map (layer: layer.drvPath);
  checksOf = build: {
    drvPath = build.drvPath;
    reusedStubLayers = map (check: derivationsOf check.reusedStubLayers) build.stubLayerReuseChecks;
  };
in
{
  registry = checksOf reference.registryArtifacts;
  engine = checksOf reference.engineArtifacts;
  unified = checksOf reference.unifiedArtifacts;
  framework = checksOf reference.frameworkArtifacts;
  package = checksOf reference;
}
