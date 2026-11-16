# SPDX-License-Identifier: GPL-3.0-only
{
  lib,
  bindings,
  generated,
  supportedFeatures,
  cargoLock,
  workspaces,
}:
let
  inherit (import ./cargo-manifest.nix { inherit lib; }) dependencyTables;
  lockedForkCrates = lib.filter (
    package: lib.elem package.name bindings.crates
  ) (lib.importTOML cargoLock).package;
  lockedBindingsVersions = map (package: package.version) (
    lib.filter (package: package.name == "skia-bindings") lockedForkCrates
  );
  dependencyOptions = declaration: if builtins.isString declaration then { } else declaration;
  skiaSafeDeclarations =
    root:
    let
      manifest = lib.importTOML (root + "/Cargo.toml");
      members = map (
        member:
        assert lib.assertMsg (!lib.hasInfix "*" member) ''
          The workspace member ${member} is a glob, so the skia-safe features of its crates cannot be read.
          List the members of the workspace by their paths.'';
        lib.importTOML (root + "/${member}/Cargo.toml")
      ) (manifest.workspace.members or [ ]);
      shared = dependencyOptions (manifest.workspace.dependencies.skia-safe or { });
      declarations = lib.concatMap (
        toml:
        map (dependencies: dependencyOptions dependencies.skia-safe) (
          lib.filter (dependencies: dependencies ? skia-safe) (dependencyTables toml)
        )
      ) ([ manifest ] ++ members);
    in
    map (
      declaration:
      if declaration.workspace or false then
        {
          features = shared.features or [ ] ++ declaration.features or [ ];
          default = shared.default-features or true;
        }
      else
        {
          features = declaration.features or [ ];
          default = declaration.default-features or true;
        }
    ) declarations;
  declarations = lib.concatMap skiaSafeDeclarations workspaces;
  enabledFeatures = lib.sort lib.lessThan (
    lib.unique (
      lib.concatMap (declaration: declaration.features) declarations
      ++ lib.optional (lib.any (declaration: declaration.default) declarations) "default"
    )
  );
in
assert lib.assertMsg (generated.fork_rev == bindings.rev) ''
  nix/skia-source.json was generated for the commit ${generated.fork_rev} of the Skia fork,
  but ${bindings.url} is locked at ${bindings.rev}. Run nix/refresh_skia_source.py.'';
assert lib.assertMsg (lockedBindingsVersions == [ bindings.version ]) ''
  ${toString cargoLock} locks skia-bindings ${toString lockedBindingsVersions},
  but nix/skia.nix builds Skia for skia-bindings ${bindings.version}, the release locked by the Cargo.lock of Lavashell.
  Lock that release in ${toString cargoLock}. To build another release, lock it in the Cargo.lock of Lavashell, set
  the hash of its commit in nix/skia-bindings.nix, then run nix/refresh_skia_source.py.'';
assert lib.assertMsg
  (lib.all (package: (package.source or null) == bindings.cargoSource) lockedForkCrates)
  ''
    ${toString cargoLock} locks ${
      lib.concatMapStringsSep " and " (
        package: "${package.name} from ${package.source or "a path"}"
      ) lockedForkCrates
    },
    but the Skia builds need ${lib.concatStringsSep " and " bindings.crates} from ${bindings.cargoSource}.
    Add these lines to the Cargo.toml of the workspace root and update the lock file:

    ${lib.concatMapStringsSep "\n\n" (crate: ''
      [patch.crates-io.${crate}]
      git = "${bindings.url}"
      rev = "${bindings.rev}"'') bindings.crates}'';
assert lib.assertMsg (enabledFeatures == supportedFeatures) ''
  The crates of ${toString workspaces} enable the skia-safe features ${toString enabledFeatures},
  but nix/skia.nix builds Skia for ${toString supportedFeatures}.
  Enable exactly those features, or give the features of nix/skia.nix one entry for each enabled
  feature, with the gn arguments, libraries and defining targets that skia-bindings derives from it.'';
true
