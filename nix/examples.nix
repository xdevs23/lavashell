# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  skia,
  rustPlatform,
  sources,
  assets,
}:
let
  inherit (pkgs) lib;
  contract = import ./projection-contract.nix;
  types = import ./types.nix;
  runnable = lib.filterAttrs (
    name: type:
    type == "directory"
    && builtins.pathExists (root + "/examples/${name}/Cargo.toml")
    && builtins.pathExists (root + "/examples/${name}/src/main.rs")
  ) (builtins.readDir (root + "/examples"));
  directories = lib.mapAttrs (name: _: root + "/examples/${name}") runnable;
  packagedDirectories = lib.mapAttrs (_: directory: root + "/${directory}") contract.packagedCrates;
  packageDirectoriesByName = lib.attrsets.unionOfDisjoint directories packagedDirectories;
  packageDirectories = lib.attrValues packageDirectoriesByName;
  packageRoots = map sources.packageName packageDirectories;
  launches = {
    wayland-input = {
      kind = types.launchKinds.helper;
      arguments = [ "--keyboard" ];
    };
  };
  notices = import ./notices.nix {
    inherit
      pkgs
      sources
      root
      rustPlatform
      ;
  };
  rustPackageDefaults = import ./rust-package.nix {
    inherit pkgs assets;
    inherit (sources) version;
  };
  requirementsOf = import ./crate-requirements.nix { inherit pkgs skia assets; };
  layerTools = import ./build-layers.nix {
    inherit pkgs root rustPlatform;
    cargoWorkspace = sources.cargoWorkspaces.buildLayers;
  };
  buildLayers = layerTools.tools;
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  artifacts = import ./artifacts.nix {
    inherit
      pkgs
      rustPlatform
      sources
      rustPackageDefaults
      requirementsOf
      buildLayers
      ;
  };
  inherit (sources) nativeDirectory;
  registry = artifacts.registry nativeDirectory;
  framework = artifacts.framework nativeDirectory [ registry ] { example = false; };
  buildPackage =
    name: directory:
    let
      program = sources.packageName directory;
      library = !builtins.pathExists (directory + "/src/main.rs");
      crates = sources.localCrates directory;
      closure = closures.${name};
      dependencies = lib.remove program closure;
      route = sources.layerRoute {
        package = program;
        inherit crates;
      };
      withEngine = route == types.layerRoutes.frameworkWithEngine;
      engine = if withEngine then artifacts.engine directory else null;
      requirements = requirementsOf dependencies;
      testedByOtherPackages =
        lib.remove program packageRoots
        ++ lib.concatMap (package: package.testedPackages) (
          packagesTestingBefore name (
            lib.subtractLists (packageRoots ++ artifacts.packagesTestedThrough [ framework ]) closure
          )
        );
      routed =
        {
          ${types.layerRoutes.registryOnly} = artifacts.registry directory;
          ${types.layerRoutes.framework} = framework;
          ${types.layerRoutes.frameworkWithEngine} = artifacts.framework directory [ framework engine ] {
            validateEngine = true;
            inherit testedByOtherPackages;
          };
        }
        .${route};
      local =
        if builtins.elem name contract.libraryLayers then
          artifacts.framework directory [ routed ] {
            example = false;
            features = sources.dependencyFeaturesEnabledByDependents {
              inherit directory;
              dependents = lib.filter (
                other: other != directory && builtins.elem program (sources.packageNames other)
              ) packageDirectories;
            };
            testedByOtherPackages = [ program ] ++ testedByOtherPackages;
          }
        else
          routed;
      libraries = map (library: library.frameworkArtifacts) (
        lib.filter (
          library: builtins.elem (sources.packageName library.directory) dependencies
        ) libraryCrates
      );
      selected = artifacts.selectedPackages directory;
      testPackages = lib.subtractLists (
        artifacts.packagesTestedThrough [
          framework
          local
        ]
        ++ testedByOtherPackages
      ) closure;
      testedPackages = testPackages ++ artifacts.packagesTestedThrough [ local ];
    in
    rustPlatform.buildRustPackage (
      rustPackageDefaults
      // artifacts.tests testPackages
      // artifacts.lint testPackages
      // artifacts.enableFeatures local.cargoBuildFeatures
      // {
        pname = "lavashell-${name}";
        inherit (sources.sourceAndCargoDeps directory) src cargoDeps;
        buildInputs =
          rustPackageDefaults.buildInputs
          ++ lib.concatMap (requirement: requirement.buildInputs) requirements;
        nativeCheckInputs =
          rustPackageDefaults.nativeCheckInputs
          ++ lib.concatMap (requirement: requirement.nativeCheckInputs) (requirementsOf testPackages);
        cargoBuildFlags =
          artifacts.flags selected
          ++ [ "--frozen" ]
          ++ lib.optionals (!library) [
            "--bin"
            program
          ]
          ++ rustPackageDefaults.cargoBuildFlags;
        postInstall =
          rustPackageDefaults.postInstall
          + ''
            install -Dm444 ${notices { inherit directory; }} "$out/share/licenses/lavashell/rust-crates.txt"
          ''
          + lib.concatMapStrings (requirement: requirement.installLicenses) requirements;
        preBuild =
          rustPackageDefaults.preBuild
          + (import ./vendor-directory-times.nix)
          + artifactLayers.restore ([ local ] ++ libraries)
          + lib.optionalString withEngine (artifacts.planCheck directory selected);
        passthru = {
          inherit
            directory
            route
            testPackages
            testedPackages
            ;
          registryArtifacts = registry;
          engineArtifacts = engine;
          frameworkArtifacts = local;
          libraryArtifacts = libraries;
        };
        meta = rustPackageDefaults.meta // lib.optionalAttrs (!library) { mainProgram = program; };
      }
    );
  closures = lib.mapAttrs (
    _: directory: sources.crateNames (sources.localCrates directory)
  ) packageDirectoriesByName;
  testOrder = lib.mapAttrs (name: closure: [
    (lib.length closure)
    name
  ]) closures;
  packagesTestingBefore =
    name: crates:
    lib.attrValues (
      lib.filterAttrs (
        other: _:
        testOrder.${other} < testOrder.${name} && lib.intersectLists crates closures.${other} != [ ]
      ) packagesByName
    );
  packagesByName = lib.mapAttrs buildPackage packageDirectoriesByName;
  packagedCrates = lib.getAttrs (lib.attrNames packagedDirectories) packagesByName;
  libraryCrates = lib.attrValues (lib.getAttrs contract.libraryLayers packagedCrates);
  packages = lib.getAttrs (lib.attrNames directories) packagesByName // {
    inherit (packagedCrates) wayland-input;
  };
  examples = pkgs.symlinkJoin {
    name = "lavashell-examples-${sources.version}";
    paths = lib.attrValues packages;
    passthru.packages = lib.attrValues packages;
    meta = rustPackageDefaults.meta;
  };
in
{
  reference = packagedCrates.bevy;
  inherit buildLayers packagedCrates;
  packages = packages // {
    inherit examples;
  };
  apps = lib.mapAttrs (name: value: {
    type = "app";
    program = lib.getExe value;
    meta.description = value.meta.description;
    meta.lavashellHost = types.hosts.wayland;
    meta.lavashellLaunch =
      launches.${name} or {
        kind = types.launchKinds.window;
        arguments = [ ];
      };
  }) packages;
  checks =
    let
      toolPython = pkgs.python3.withPackages (python: [
        python.mypy
        python.psutil
        python.tomli-w
        python.types-psutil
      ]);
      withoutDefaultFeatures = import ./without-default-features.nix {
        inherit
          pkgs
          rustPlatform
          root
          sources
          rustPackageDefaults
          skia
          ;
      };
    in
    packagedCrates
    // {
      inherit registry framework examples;
      nix-tests = layerTools.nixTests;
      effect-removal = withoutDefaultFeatures {
        pname = "lavashell-effect-removal";
        crate = "lavashell-renderer";
        subcommand = "run";
        arguments = [
          "--example"
          "independence-probe"
        ];
      };
      desktop-removal = withoutDefaultFeatures {
        pname = "lavashell-desktop-removal";
        crate = "lavashell";
        subcommand = "clippy";
        arguments = [
          "--"
          "--deny"
          "warnings"
        ];
      };
      source-limits =
        pkgs.runCommand "lavashell-source-limits"
          {
            nativeBuildInputs = [
              toolPython
              pkgs.ruff
              rustPlatform.rust.cargo
              packagedCrates.source-rules
            ];
          }
          ''
            cd ${root}
            lavashell-source-rules
            ruff check --no-cache --config tools/ruff.toml nix
            mypy --config-file tools/mypy.toml --cache-dir=/dev/null nix/*.py
            touch "$out"
          '';
      python-tests =
        pkgs.runCommand "lavashell-python-tests"
          {
            nativeBuildInputs = [
              toolPython
              pkgs.nix
            ];
            env = {
              PYTHONDONTWRITEBYTECODE = "1";
              LAVASHELL_NIXPKGS = "${pkgs.path}";
            };
          }
          ''
            export HOME="$TMPDIR" NIX_STORE_DIR="$TMPDIR/store" NIX_STATE_DIR="$TMPDIR/state" NIX_LOG_DIR="$TMPDIR/log"
            cd ${root}
            python3 -m unittest discover -s nix -p 'test_*.py'
            touch "$out"
          '';
    };
}
