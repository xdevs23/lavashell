# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  commits,
}:
let
  inherit (pkgs) lib;
  workspaceCrates = import ./workspace-crates.nix { inherit lib root; };
  lock = import ./cargo-lock.nix { inherit lib root; };
  toml = import ./cargo-toml.nix { inherit lib; };
  targets = import ./cargo-targets.nix { inherit lib; };
  dependencyWorkspace = import ./dependency-workspace.nix { inherit root toml; };
  projection = import ./projection.nix {
    inherit
      lib
      root
      toml
      workspaceCrates
      lock
      targets
      ;
  };
  inherit (projection) metadataFor nativeMetadata;
  source = import ./source-tree.nix {
    inherit
      pkgs
      root
      toml
      workspaceCrates
      targets
      ;
    inherit (projection) compileManifest;
  };
  inherit (workspaceCrates) nativeDirectory;
  types = import ./types.nix;
  dependencyProject =
    directory: layer:
    let
      metadata =
        if toString directory == toString nativeDirectory then nativeMetadata else metadataFor directory;
    in
    if metadata == null then
      throw "Run nix run .#refresh-cargo-projections for ${workspaceCrates.packageName directory}"
    else
      dependencyWorkspace {
        inherit layer metadata;
        seedHash = (projection.seedPlan directory layer).seedHash;
        target = pkgs.stdenv.hostPlatform.rust.rustcTarget;
      };
  engineProject = directory: dependencyProject directory types.layers.engine;
  registryProject = directory: dependencyProject directory types.layers.registry;
in
{
  inherit (projection)
    project
    layerRoute
    seedPlan
    ;
  inherit (workspaceCrates)
    nativeDirectory
    nativeNames
    packageName
    packageNames
    localCrates
    crateNames
    dependencyFeaturesEnabledByDependents
    ;
  version = workspaceCrates.workspace.workspace.package.version;
  inherit dependencyProject engineProject registryProject;
  vendor = import ./cargo-vendor.nix { inherit pkgs commits; };
  inherit metadataFor;
  sourceFor = directory: source (projection.project directory) [ ];
  registrySource = directory: source (registryProject directory) (registryProject directory).members;
  engineSource = directory: source (engineProject directory) (engineProject directory).members;
  frameworkSource =
    directory: source (projection.project directory) [ (workspaceCrates.relative directory) ];
}
