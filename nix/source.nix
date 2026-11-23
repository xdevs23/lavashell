# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  projections,
  commits,
}:
let
  inherit (pkgs) lib;
  workspaceCrates = import ./workspace-crates.nix { inherit lib root; };
  lock = import ./cargo-lock.nix { inherit lib root; };
  toml = import ./cargo-toml.nix { inherit lib; };
  targets = import ./cargo-targets.nix { inherit lib; };
  packageDirectories = import ./package-directories.nix { inherit lib root; };
  packageTests = import ./package-tests.nix { inherit lib workspaceCrates packageDirectories; };
  projection = import ./projection.nix {
    inherit
      lib
      root
      projections
      toml
      workspaceCrates
      packageTests
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
  vendor = import ./cargo-vendor.nix { inherit pkgs commits; };
  withCargoDeps = projected: projected // { cargoDeps = vendor projected.lockFile; };
  inherit (workspaceCrates) memoizePerMember;
  project = memoizePerMember (directory: withCargoDeps (projection.project directory));
  sourceFor = memoizePerMember (directory: source (project directory) [ ]);
  dependencyProject =
    directory: layer:
    let
      metadata =
        if toString directory == toString nativeDirectory then nativeMetadata else metadataFor directory;
    in
    if metadata == null then
      throw "Run nix run .#refresh-cargo-projections for ${workspaceCrates.packageName directory}"
    else
      import ./seed-hash-check.nix {
        inherit layer metadata;
        target = pkgs.stdenv.hostPlatform.rust.rustcTarget;
        seedProject = projection.seedProject directory layer;
      };
  engineProject = memoizePerMember (
    directory: withCargoDeps (dependencyProject directory types.layers.engine)
  );
  registryProject = memoizePerMember (
    directory: withCargoDeps (dependencyProject directory types.layers.registry)
  );
  stubLayer =
    projectOf:
    memoizePerMember (
      directory:
      let
        projected = projectOf directory;
      in
      {
        projection = projected;
        source = source projected projected.members;
      }
    );
in
{
  inherit (projection)
    layerRouteOf
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
  inherit
    project
    packageDirectories
    packageTests
    ;
  cargoWorkspaces = lib.mapAttrs (_: directory: {
    inherit directory;
    cargoDeps = vendor (root + "/${directory}/Cargo.lock");
  }) (lib.importTOML ./cargo-workspaces.toml).directories;
  inherit sourceFor;
  sourceAndCargoDeps = directory: {
    src = sourceFor directory;
    inherit (project directory) cargoDeps;
  };
  stubLayers = {
    registry = stubLayer registryProject;
    engine = stubLayer engineProject;
    unified = stubLayer project;
  };
  frameworkSource = memoizePerMember (
    directory: source (project directory) [ (workspaceCrates.relative directory) ]
  );
}
