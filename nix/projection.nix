# SPDX-License-Identifier: GPL-3.0-only
{
  lib,
  root,
  projections,
  toml,
  workspaceCrates,
  lock,
  targets,
}:
let
  contract = import ./projection-contract.nix;
  types = import ./types.nix;
  inherit (workspaceCrates)
    workspace
    expandDependency
    relative
    memoizePerMember
    packageName
    crateNames
    localCrates
    nativeDirectory
    nativeNames
    buildScript
    externalDependencies
    dependencyKinds
    dependencyTables
    ;
  compileManifest =
    manifest:
    (lib.filterAttrs (
      name: _:
      builtins.elem name (
        dependencyKinds
        ++ [
          "features"
          "target"
          "lib"
          "bin"
          "test"
          "bench"
          "example"
        ]
      )
    ) manifest)
    // {
      package = lib.filterAttrs (
        name: _:
        builtins.elem name [
          "name"
          "version"
          "edition"
          "rust-version"
          "links"
          "build"
          "autobins"
          "autotests"
          "autoexamples"
          "autobenches"
        ]
      ) manifest.package;
    };
  projectCrates =
    crates: lockData:
    let
      members = map (crate: relative crate.key) crates;
      used = lib.unique (
        lib.concatMap (
          crate:
          lib.concatMap (
            table:
            lib.attrNames (
              lib.filterAttrs (
                _: dependency: builtins.isAttrs dependency && (dependency.workspace or false)
              ) table
            )
          ) (dependencyTables crate.manifest)
        ) crates
      );
      manifest = {
        workspace = workspace.workspace // {
          inherit members;
          dependencies = lib.getAttrs used workspace.workspace.dependencies;
          package = lib.getAttrs (lib.unique (
            lib.concatMap (
              crate:
              lib.attrNames (
                lib.filterAttrs (
                  _: value: builtins.isAttrs value && (value.workspace or false)
                ) crate.manifest.package
              )
            ) crates
          )) workspace.workspace.package;
        };
      }
      // lib.filterAttrs (
        name: _:
        builtins.elem name [
          "profile"
          "replace"
        ]
      ) workspace
      // lib.optionalAttrs (patch != { }) { inherit patch; };
      patch = import ./cargo-patch.nix { inherit lib; } (workspace.patch or { }) lockData;
      lockFile = builtins.toFile "Cargo.lock" (toml.lock lockData);
    in
    {
      inherit
        crates
        members
        manifest
        lockFile
        ;
    };
  metadataFor = memoizePerMember (
    directory:
    let
      file = projections + "/${packageName directory}.json";
    in
    if builtins.pathExists file then builtins.fromJSON (builtins.readFile file) else null
  );
  nativeMetadata = builtins.fromJSON (builtins.readFile (projections + "/native.json"));
  project = memoizePerMember (
    directory:
    let
      crates = localCrates directory;
      metadata = metadataFor directory;
      baseline = if metadata == null then nativeMetadata.lock else metadata.normalLock;
    in
    projectCrates crates (lock.projected baseline (crateNames crates))
  );
  withoutNative =
    manifest:
    let
      prune =
        table:
        lib.filterAttrs (
          name: dependency:
          let
            expanded = expandDependency name dependency;
            package = if builtins.isAttrs expanded then expanded.package or name else name;
          in
          !(builtins.elem package nativeNames)
        ) table;
    in
    manifest
    // lib.genAttrs (lib.filter (kind: builtins.hasAttr kind manifest) dependencyKinds) (
      kind: prune manifest.${kind}
    )
    // lib.optionalAttrs (manifest ? target) {
      target = lib.mapAttrs (_: withoutNative) manifest.target;
    };
  engineCrates =
    directory:
    lib.filter (crate: !(builtins.elem crate.manifest.package.name nativeNames)) (
      localCrates directory
    );
  engineStub =
    metadata: rootName: crate:
    let
      retained = withoutNative crate.manifest;
      rootCrate = retained.package.name == rootName;
      own = buildScript crate;
      script =
        if own != null then
          own
        else if rootCrate && metadata.features ? build-dependencies then
          contract.stub.buildScript
        else
          null;
      base =
        lib.filterAttrs (
          name: _:
          builtins.elem name (
            dependencyKinds
            ++ [
              "features"
              "target"
            ]
          )
        ) retained
        // {
          package = {
            name = retained.package.name;
            inherit (contract.stub) version;
            inherit (workspace.workspace.package) edition;
            autobins = false;
            build = if script == null then false else script;
          };
          lib = {
            path = contract.stub.library;
          }
          // lib.optionalAttrs (retained.lib.proc-macro or false) {
            proc-macro = true;
          };
        };
    in
    crate
    // {
      stubTargets = [
        {
          path = contract.stub.library;
          main = false;
        }
      ]
      ++ lib.optional (script != null) {
        path = script;
        main = true;
      };
      manifest =
        if rootCrate then
          lib.foldl' (
            manifest: kind:
            manifest
            // {
              ${kind} = (manifest.${kind} or { }) // metadata.features.${kind};
            }
          ) base (lib.attrNames metadata.features)
        else
          base;
    };
  engineSeedProject =
    directory:
    let
      metadata = metadataFor directory;
      rootName = packageName directory;
      crates = map (engineStub metadata rootName) (engineCrates directory);
    in
    if metadata == null then
      throw "Run nix run .#refresh-cargo-projections for ${rootName}"
    else
      projectCrates crates (
        lock.engine (
          metadata.engineLock
          // {
            package = map (
              package: package // lib.optionalAttrs (!(package ? source)) { inherit (contract.stub) version; }
            ) metadata.engineLock.package;
          }
        ) (crateNames crates)
      );
  plan = projectedWorkspace: {
    manifest = toml.manifest projectedWorkspace.manifest;
    lock = builtins.readFile projectedWorkspace.lockFile;
    crates = map (crate: {
      member = relative crate.key;
      manifest = toml.manifest (compileManifest crate.manifest);
      targets = targets crate;
    }) projectedWorkspace.crates;
  };
  testedByLayerAbove =
    directory: layer:
    lib.optionals (
      layer == types.layers.registry && toString directory == toString nativeDirectory
    ) nativeNames;
  seedPlan =
    directory: layer:
    let
      seed =
        plan (if layer == types.layers.engine then engineSeedProject directory else project directory)
        // {
          testedByLayerAbove = testedByLayerAbove directory layer;
        }
        // lib.optionalAttrs (layer == types.layers.engine) {
          originalContracts = map (crate: {
            member = relative crate.key;
            links = crate.manifest.package.links or null;
            build =
              crate.manifest.package.build
                or (if builtins.pathExists (builtins.toPath (crate.key + "/build.rs")) then "build.rs" else false);
          }) (engineCrates directory);
        };
    in
    seed // { seedHash = builtins.hashString "sha256" (builtins.toJSON seed); };
  nativeRegistry = map (package: package.name) (
    lib.filter (package: package ? source) nativeMetadata.lock.package
  );
  frameworkRoot =
    lib.throwIfNot (builtins.elem contract.frameworkRoot nativeNames)
      "frameworkRoot ${contract.frameworkRoot} in nix/projection-contract.json is not a crate that ${contract.nativeDirectory} depends on"
      contract.frameworkRoot;
  layerRoute =
    { package, crates }:
    let
      names = crateNames crates;
      needsEngine =
        lib.any (dependency: !(builtins.elem dependency nativeRegistry)) (externalDependencies crates)
        || lib.any (name: name != package && !(builtins.elem name nativeNames)) names;
    in
    if !(builtins.elem frameworkRoot names) then
      types.layerRoutes.registryOnly
    else if needsEngine then
      types.layerRoutes.frameworkWithEngine
    else
      types.layerRoutes.framework;
in
{
  inherit
    compileManifest
    metadataFor
    nativeMetadata
    project
    plan
    seedPlan
    layerRoute
    ;
}
