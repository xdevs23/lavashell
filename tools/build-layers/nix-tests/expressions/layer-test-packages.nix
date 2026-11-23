# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  pkgs = import input.nixpkgs { };
  inherit (pkgs) lib;
  root = /. + input.root;
  tree =
    (import (/. + input.nix + "/examples-tree.nix") (
      import (/. + input.nix + "/sources-and-assets.nix") { inherit pkgs root; }
      // {
        inherit pkgs root;
        inherit (pkgs) rustPlatform;
      }
    )).examples;
  workspaceCrates = import (/. + input.nix + "/workspace-crates.nix") { inherit lib root; };
  contract = import (/. + input.nix + "/projection-contract.nix");
  types = import (/. + input.nix + "/types.nix");
  examples = removeAttrs tree.packages [ "examples" ] // tree.packagedCrates;
  checkOf = built: {
    inherit (built) drvPath doCheck cargoTestFlags;
    cargoClippyFlags = built.cargoClippyFlags or [ ];
  };
  artifactLayers = import (/. + input.nix + "/artifact-layers.nix") { inherit lib; };
  layersThrough = layer: artifactLayers.stack [ layer ];
  layersRestoring =
    layer: package:
    [ package ]
    ++ lib.filter (above: above != layer && builtins.elem layer (layersThrough above)) (
      layersThrough package.frameworkArtifacts
    );
  stubLayerOf = package: layer: {
    layer = checkOf layer;
    layersAbove = map checkOf (layersRestoring layer package);
  };
  stubLayersCompilingTestsOf =
    package:
    if package.route == types.layerRoutes.registryOnly then
      lib.filter (layer: layer.stubLayer) (layersThrough package.frameworkArtifacts)
    else
      lib.remove null [ package.unifiedArtifacts ];
  withLayers =
    built:
    [ built ] ++ lib.optionals (built ? frameworkArtifacts) (layersThrough built.frameworkArtifacts);
  checkedDerivations = lib.concatMap (
    check: lib.concatMap withLayers ([ check ] ++ check.packages or [ ])
  ) (lib.attrValues tree.checks);
in
{
  inherit (workspaceCrates) nativeNames;
  inherit (import (/. + input.nix + "/cargo-artifact-record.nix")) messageFormat;
  members = map (
    member: workspaceCrates.packageName (root + "/${member}")
  ) workspaceCrates.workspace.workspace.members;
  checks = map checkOf (lib.filter (built: built ? cargoTestFlags) checkedDerivations);
  dependencyLayers = map checkOf (
    [ tree.checks.registry ]
    ++ lib.concatMap (
      package:
      lib.remove null [
        package.engineArtifacts
        package.unifiedArtifacts
      ]
    ) (lib.attrValues examples)
  );
  registry = checkOf tree.checks.registry;
  support = checkOf tree.checks.framework;
  libraryLayers = map (
    name:
    let
      crate = tree.packagedCrates.${name};
    in
    {
      package = workspaceCrates.packageName crate.directory;
      layer = crate.frameworkArtifacts.drvPath;
      hasEngine = crate.engineArtifacts != null;
    }
  ) contract.libraryLayers;
  examples = lib.mapAttrs (_: package: {
    package = workspaceCrates.packageName package.directory;
    closure = workspaceCrates.packageNames package.directory;
    derivations = map checkOf ([ package ] ++ layersThrough package.frameworkArtifacts);
    libraryLayers = map (layer: layer.drvPath) package.libraryArtifacts;
    hasEngine = package.engineArtifacts != null;
    stubLayers = map (stubLayerOf package) (stubLayersCompilingTestsOf package);
  }) examples;
}
