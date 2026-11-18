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
  examples = removeAttrs tree.packages [ "examples" ] // tree.packagedCrates;
  checkOf = built: {
    inherit (built) drvPath doCheck cargoTestFlags;
    cargoClippyFlags = built.cargoClippyFlags or [ ];
  };
  layersThrough = layer: [ layer ] ++ lib.concatMap layersThrough layer.previous;
  withLayers =
    built:
    [ built ] ++ lib.optionals (built ? frameworkArtifacts) (layersThrough built.frameworkArtifacts);
  checkedDerivations = lib.concatMap (
    check: lib.concatMap withLayers ([ check ] ++ check.packages or [ ])
  ) (lib.attrValues tree.checks);
in
{
  inherit (workspaceCrates) nativeNames;
  members = map (
    member: workspaceCrates.packageName (root + "/${member}")
  ) workspaceCrates.workspace.workspace.members;
  checks = map checkOf (lib.filter (built: built ? cargoTestFlags) checkedDerivations);
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
