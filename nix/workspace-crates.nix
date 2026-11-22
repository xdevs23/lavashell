# SPDX-License-Identifier: GPL-3.0-only
{ lib, root }:
let
  contract = import ./projection-contract.nix;
  cargoManifest = import ./cargo-manifest.nix { inherit lib; };
  inherit (cargoManifest) dependencyTables;
  workspace = builtins.fromTOML (builtins.readFile (root + "/Cargo.toml"));
  dependencies = manifest: lib.foldl' (a: b: a // b) { } (dependencyTables manifest);
  expandDependency =
    name: dependency:
    let
      named = if builtins.isAttrs dependency then dependency // { inherit name; } else dependency;
    in
    if builtins.isAttrs named && (named.workspace or false) then
      let
        inherited = workspace.workspace.dependencies.${named.name};
      in
      (if builtins.isAttrs inherited then inherited else { version = inherited; })
      // lib.removeAttrs named [
        "name"
        "workspace"
      ]
    else
      named;
  relative = path: lib.removePrefix "${toString root}/" (toString path);
  memoizePerMember =
    compute:
    let
      computed = lib.genAttrs workspace.workspace.members (member: compute (root + "/${member}"));
    in
    directory:
    let
      member = relative directory;
    in
    lib.throwIfNot (computed ? ${member})
      "${member} must be a workspace member of the root Cargo.toml, so add it to workspace.members there"
      computed.${member};
  packageName = directory: (readManifest directory).package.name;
  readManifest = directory: builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"));
  crateAt = directory: {
    key = toString directory;
    manifest = readManifest directory;
  };
  crateNames = map (crate: crate.manifest.package.name);
  localCrates =
    directory:
    lib.genericClosure {
      startSet = [ (crateAt directory) ];
      operator =
        crate:
        lib.mapAttrsToList
          (
            name: dependency:
            let
              declared = (dependencies crate.manifest).${name};
              path =
                if declared.workspace or false then
                  root + "/${dependency.path}"
                else
                  builtins.toPath (crate.key + "/${dependency.path}");
            in
            crateAt path
          )
          (
            lib.filterAttrs (_: dependency: builtins.isAttrs dependency && dependency ? path) (
              lib.mapAttrs expandDependency (dependencies crate.manifest)
            )
          );
    };
  nativeDirectory = root + "/${contract.nativeDirectory}";
  dependencyFeaturesEnabledByDependents =
    { directory, dependents }:
    let
      declaredByCrate = builtins.intersectAttrs ((readManifest directory).dependencies or { });
      featuresOf =
        dependency: declared: map (feature: "${dependency}/${feature}") (declared.features or [ ]);
    in
    lib.unique (
      lib.concatMap (table: lib.concatLists (lib.mapAttrsToList featuresOf (declaredByCrate table))) (
        lib.concatMap (dependent: dependencyTables (readManifest dependent)) dependents
      )
    );
in
{
  inherit
    workspace
    expandDependency
    relative
    memoizePerMember
    packageName
    crateAt
    crateNames
    localCrates
    nativeDirectory
    dependencyFeaturesEnabledByDependents
    ;
  inherit (cargoManifest) dependencyKinds dependencyTables;
  packageNames = directory: crateNames (localCrates directory);
  nativeNames = crateNames (localCrates nativeDirectory);
  buildScript =
    crate:
    let
      declared = crate.manifest.package.build or true;
    in
    if builtins.isString declared then
      declared
    else if declared == true && builtins.pathExists (builtins.toPath (crate.key + "/build.rs")) then
      "build.rs"
    else
      null;
  externalDependencies =
    crates:
    lib.concatMap (
      crate:
      lib.concatMap (
        table:
        lib.concatLists (
          lib.mapAttrsToList (
            name: dependency:
            let
              expanded = expandDependency name dependency;
            in
            if builtins.isAttrs expanded && expanded ? path then
              [ ]
            else if builtins.isAttrs expanded then
              [ (expanded.package or name) ]
            else
              [ name ]
          ) table
        )
      ) (dependencyTables crate.manifest)
    ) crates;
}
