# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root ? ../.,
}:
let
  inherit (pkgs) lib;
  workspace = builtins.fromTOML (builtins.readFile (root + "/Cargo.toml"));
  lock = import ./cargo-lock.nix { inherit lib root; };
  toml = import ./cargo-toml.nix { inherit lib; };
  dependencyWorkspace = import ./dependency-workspace.nix { inherit root toml; };
  tables =
    manifest:
    map (kind: manifest.${kind} or { }) [
      "dependencies"
      "dev-dependencies"
      "build-dependencies"
    ]
    ++ lib.concatMap tables (builtins.attrValues (manifest.target or { }));
  dependencies = manifest: lib.foldl' (a: b: a // b) { } (tables manifest);
  expand =
    dependency:
    if builtins.isAttrs dependency && (dependency.workspace or false) then
      let
        inherited = workspace.workspace.dependencies.${dependency.name};
      in
      (if builtins.isAttrs inherited then inherited else { version = inherited; })
      // lib.removeAttrs dependency [
        "name"
        "workspace"
      ]
    else
      dependency;
  localCrates =
    directory:
    lib.genericClosure {
      startSet = [
        {
          key = toString directory;
          manifest = builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"));
        }
      ];
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
            {
              key = toString path;
              manifest = builtins.fromTOML (builtins.readFile (path + "/Cargo.toml"));
            }
          )
          (
            lib.filterAttrs (_: dependency: builtins.isAttrs dependency && dependency ? path) (
              lib.mapAttrs (
                name: dependency:
                expand (if builtins.isAttrs dependency then dependency // { inherit name; } else dependency)
              ) (dependencies crate.manifest)
            )
          );
    };
  relative = path: lib.removePrefix "${toString root}/" (toString path);
  compileManifest =
    manifest:
    (lib.filterAttrs (
      name: _:
      builtins.elem name [
        "dependencies"
        "dev-dependencies"
        "build-dependencies"
        "features"
        "target"
        "lib"
        "bin"
        "test"
        "bench"
        "example"
      ]
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
          ) (tables crate.manifest)
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
  nativeDirectory = root + "/examples/support";
  nativeNames = map (crate: crate.manifest.package.name) (localCrates nativeDirectory);
  metadataFor =
    directory:
    let
      name = (builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"))).package.name;
      file = root + "/nix/cargo-projections/${name}.json";
    in
    if builtins.pathExists file then builtins.fromJSON (builtins.readFile file) else null;
  nativeMetadata = builtins.fromJSON (
    builtins.readFile (root + "/nix/cargo-projections/native.json")
  );
  project =
    directory:
    let
      crates = localCrates directory;
      names = map (crate: crate.manifest.package.name) crates;
      metadata = metadataFor directory;
      baseline = if metadata == null then nativeMetadata.lock else metadata.normalLock;
    in
    projectCrates crates (lock.projected baseline names);
  withoutNative =
    manifest:
    let
      prune =
        table:
        lib.filterAttrs (
          name: dependency:
          let
            expanded = expand (
              if builtins.isAttrs dependency then dependency // { inherit name; } else dependency
            );
            package = if builtins.isAttrs expanded then expanded.package or name else name;
          in
          !(builtins.elem package nativeNames)
        ) table;
    in
    manifest
    // lib.genAttrs (lib.filter (kind: builtins.hasAttr kind manifest) [
      "dependencies"
      "dev-dependencies"
      "build-dependencies"
    ]) (kind: prune manifest.${kind})
    // lib.optionalAttrs (manifest ? target) {
      target = lib.mapAttrs (_: withoutNative) manifest.target;
    };
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
  engineSeedProject =
    directory:
    let
      metadata = metadataFor directory;
      rootName = (builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"))).package.name;
      crates =
        map
          (
            crate:
            let
              retained = withoutNative crate.manifest;
              rootCrate = retained.package.name == rootName;
              own = buildScript crate;
              script =
                if own != null then
                  own
                else if rootCrate && metadata.features ? build-dependencies then
                  "build.rs"
                else
                  null;
              base =
                lib.filterAttrs (
                  name: _:
                  builtins.elem name [
                    "dependencies"
                    "dev-dependencies"
                    "build-dependencies"
                    "features"
                    "target"
                  ]
                ) retained
                // {
                  package = {
                    name = retained.package.name;
                    version = "0.0.0";
                    edition = "2024";
                    autobins = false;
                    build = if script == null then false else script;
                  };
                  lib = {
                    path = "src/lib.rs";
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
                  path = "src/lib.rs";
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
            }
          )
          (
            lib.filter (crate: !(builtins.elem crate.manifest.package.name nativeNames)) (localCrates directory)
          );
    in
    if metadata == null then
      throw "Run nix/refresh-cargo-projections.py for ${rootName}"
    else
      projectCrates crates (
        lock.engine (
          metadata.engineLock
          // {
            package = map (
              package: package // lib.optionalAttrs (!(package ? source)) { version = "0.0.0"; }
            ) metadata.engineLock.package;
          }
        ) (map (crate: crate.manifest.package.name) crates)
      );
  plan = directory: data: {
    manifest = toml.manifest data.manifest;
    lock = builtins.readFile data.lockFile;
    crates = map (crate: {
      member = relative crate.key;
      manifest = toml.manifest (compileManifest crate.manifest);
      targets = targets crate;
    }) data.crates;
  };
  seedPlan =
    directory: layer:
    let
      data =
        plan directory (if layer == "engine" then engineSeedProject directory else project directory)
        // lib.optionalAttrs (layer == "engine") {
          originalContracts =
            map
              (crate: {
                member = relative crate.key;
                links = crate.manifest.package.links or null;
                build =
                  crate.manifest.package.build
                    or (if builtins.pathExists (builtins.toPath (crate.key + "/build.rs")) then "build.rs" else false);
              })
              (
                lib.filter (crate: !(builtins.elem crate.manifest.package.name nativeNames)) (localCrates directory)
              );
        };
    in
    data // { seedHash = builtins.hashString "sha256" (builtins.toJSON data); };
  dependencyProject =
    directory: layer:
    let
      metadata =
        if toString directory == toString nativeDirectory then nativeMetadata else metadataFor directory;
    in
    dependencyWorkspace {
      inherit layer;
      metadata = if metadata == null then { } else metadata;
      seedHash = (seedPlan directory layer).seedHash;
      target = pkgs.stdenv.hostPlatform.rust.rustcTarget;
    };
  engineProject = directory: dependencyProject directory "engine";
  registryProject = directory: dependencyProject directory "registry";
  targets = import ./cargo-targets.nix { inherit lib; };
  source =
    data: dummyMembers:
    let
      real = lib.subtractLists dummyMembers data.members;
      src = lib.cleanSourceWith {
        name = "source";
        src = root;
        filter =
          path: type:
          let
            name = relative path;
          in
          lib.cleanSourceFilter path type
          && builtins.baseNameOf path != "node_modules"
          && (
            path == toString root
            || (name == "COPYING" && real != [ ])
            || lib.any (
              member: lib.hasPrefix "${name}/" member || member == name || lib.hasPrefix "${member}/" name
            ) real
          );
      };
      manifests = lib.concatMapStrings (
        crate:
        let
          member = relative crate.key;
          contents = toml.manifest (
            if builtins.elem member dummyMembers then compileManifest crate.manifest else crate.manifest
          );
          file = builtins.toFile "Cargo.toml" contents;
        in
        ''
          mkdir -p "$out/${member}"
          cp ${file} "$out/${member}/Cargo.toml"
        ''
      ) data.crates;
      stubs = lib.concatMapStrings (
        crate:
        lib.optionalString (builtins.elem (relative crate.key) dummyMembers) (
          lib.concatMapStrings (target: ''
            mkdir -p "$out/$(dirname ${lib.escapeShellArg "${relative crate.key}/${target.path}"})"
            printf '%s\n' ${lib.escapeShellArg (if target.main then "fn main() {}" else "")} \
              > "$out/${relative crate.key}/${target.path}"
          '') (targets crate)
        )
      ) data.crates;
      manifest = builtins.toFile "Cargo.toml" (toml.manifest data.manifest);
    in
    pkgs.runCommand "source" { inherit src; } ''
      mkdir -p "$out"
      cp -R "$src/." "$out/"
      chmod -R u+w "$out"
      cp ${manifest} "$out/Cargo.toml"
      cp ${data.lockFile} "$out/Cargo.lock"
      ${manifests}
      ${stubs}
    '';
in
{
  inherit
    project
    engineProject
    registryProject
    nativeNames
    plan
    seedPlan
    ;
  vendor = import ./cargo-vendor.nix {
    inherit pkgs;
    commits = [ (import ./skia-bindings.nix) ];
  };
  needsEngine =
    directory:
    let
      crates = localCrates directory;
      name = (builtins.head crates).manifest.package.name;
      nativeRegistry = map (package: package.name) (
        lib.filter (package: package ? source) nativeMetadata.lock.package
      );
      external = lib.concatMap (
        crate:
        lib.concatMap (
          table:
          lib.mapAttrsToList (
            name: dependency:
            let
              expanded = expand (
                if builtins.isAttrs dependency then dependency // { inherit name; } else dependency
              );
            in
            if builtins.isAttrs expanded && expanded ? path then
              null
            else if builtins.isAttrs expanded then
              expanded.package or name
            else
              name
          ) table
        ) (tables crate.manifest)
      ) crates;
    in
    lib.any (dependency: dependency != null && !(builtins.elem dependency nativeRegistry)) external
    || lib.any (
      crate:
      crate.manifest.package.name != name && !(builtins.elem crate.manifest.package.name nativeNames)
    ) crates;
  engineMetadata = metadataFor;
  sourceFor = directory: source (project directory) [ ];
  registrySource = directory: source (registryProject directory) (registryProject directory).members;
  engineSource = directory: source (engineProject directory) (engineProject directory).members;
  frameworkSource = directory: source (project directory) [ (relative directory) ];
  packageNames = directory: map (crate: crate.manifest.package.name) (localCrates directory);
}
