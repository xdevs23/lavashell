# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  pkgs = import input.nixpkgs { };
  inherit (pkgs) lib;
  root = /. + input.root;
  tree =
    (import (/. + input.nix + "/examples-tree.nix") {
      inherit pkgs root;
      inherit (pkgs) rustPlatform;
    }).examples;
  workspaceCrates = import (/. + input.nix + "/workspace-crates.nix") { inherit lib root; };
  examples = removeAttrs tree.packages [ "examples" ] // {
    aosp-round-trip = tree.aospRoundTrip;
    source-rules = tree.sourceRules;
    bevy = tree.reference;
  };
  checkOf = built: {
    inherit (built) drvPath doCheck cargoTestFlags;
  };
  layersThrough = layer: [ layer ] ++ lib.concatMap layersThrough layer.previous;
in
{
  inherit (workspaceCrates) nativeNames;
  dependencyLayers = map checkOf (
    [ tree.checks.registry ]
    ++ lib.filter (engine: engine != null) (
      lib.mapAttrsToList (_: package: package.engineArtifacts) examples
    )
  );
  support = checkOf tree.checks.framework;
  examples = lib.mapAttrs (_: package: {
    closure = workspaceCrates.packageNames package.directory;
    derivations = map checkOf ([ package ] ++ layersThrough package.frameworkArtifacts);
  }) examples;
}
