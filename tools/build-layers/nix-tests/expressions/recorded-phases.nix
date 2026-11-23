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
  phasesOf = built: {
    inherit (built)
      buildPhase
      checkPhase
      clippyPhase
      cargoBuildFlags
      cargoTestFlags
      cargoClippyFlags
      ;
  };
in
{
  inherit (import (/. + input.nix + "/cargo-artifact-record.nix") { inherit pkgs; }) fileName;
  package = phasesOf reference;
  stubLayer = phasesOf reference.registryArtifacts;
}
