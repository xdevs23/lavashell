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
  inherit (sources) packageDirectories;
  inherit (sources.packageTests) closures testedCrates;
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
  inherit (import ./crate-requirements.nix { inherit pkgs skia assets; })
    requirementsOf
    buildInputsOf
    ;
  layerTools = import ./build-layers.nix {
    inherit pkgs root rustPlatform;
    cargoWorkspace = sources.cargoWorkspaces.buildLayers;
  };
  buildLayers = layerTools.tools;
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  checkStubLayerReuse = import ./stub-layer-reuse-check.nix { inherit pkgs buildLayers; };
  artifacts = import ./artifacts.nix {
    inherit
      pkgs
      rustPlatform
      sources
      rustPackageDefaults
      buildInputsOf
      buildLayers
      ;
  };
  inherit (sources) nativeDirectory;
  registry = artifacts.registry nativeDirectory;
  framework = artifacts.framework nativeDirectory [ registry ] {
    example = false;
    tested = testedCrates.${sources.packageName nativeDirectory};
  };
  buildPackage =
    name: directory:
    let
      program = sources.packageName directory;
      library = !builtins.pathExists (directory + "/src/main.rs");
      closure = closures.${program};
      tested = testedCrates.${program};
      dependencies = lib.remove program closure;
      route = sources.layerRouteOf directory;
      withEngine = route == types.layerRoutes.frameworkWithEngine;
      engine = if withEngine then artifacts.engine directory else null;
      unified =
        if withEngine then artifacts.unified directory { inherit registry engine; } tested else null;
      requirements = requirementsOf dependencies;
      routed =
        {
          ${types.layerRoutes.registryOnly} = artifacts.registry directory;
          ${types.layerRoutes.framework} = framework;
          ${types.layerRoutes.frameworkWithEngine} = artifacts.framework directory [ framework unified ] {
            inherit tested;
          };
        }
        .${route};
      withLibraryLayer = builtins.elem name contract.libraryLayers;
      featuresEnabledByDependents = lib.optionals withLibraryLayer (
        sources.dependencyFeaturesEnabledByDependents {
          inherit directory;
          dependents = lib.filter (
            other: other != directory && builtins.elem program (sources.packageNames other)
          ) (lib.attrValues packageDirectories.all);
        }
      );
      local =
        if withLibraryLayer then
          artifacts.framework directory [ routed ] {
            example = false;
            features = featuresEnabledByDependents;
            tested = lib.remove program tested;
          }
        else
          routed;
      libraries = map (library: library.frameworkArtifacts) (
        lib.filter (
          library: builtins.elem (sources.packageName library.directory) dependencies
        ) libraryCrates
      );
      restoredLayers = [ local ] ++ libraries;
      stubLayerReuseChecks = [
        (checkStubLayerReuse {
          checkedBuild = types.checkedBuilds.consumer;
          lowerLayers = restoredLayers;
        })
      ];
      inherit (sources.project directory) selectedPackages;
      sourceAndCargoDeps = sources.sourceAndCargoDeps directory;
      testPackages = lib.subtractLists (artifacts.packagesTestedThrough [
        framework
        local
      ]) tested;
    in
    rustPlatform.buildRustPackage (
      rustPackageDefaults
      // artifacts.tests testPackages
      // artifacts.lint testPackages
      // artifacts.enableFeatures featuresEnabledByDependents
      // import ./vendored-source.nix sourceAndCargoDeps
      // {
        pname = "lavashell-${name}";
        buildInputs = rustPackageDefaults.buildInputs ++ buildInputsOf dependencies;
        nativeCheckInputs =
          rustPackageDefaults.nativeCheckInputs
          ++ lib.concatMap (requirement: requirement.nativeCheckInputs) (requirementsOf testPackages);
        cargoBuildFlags =
          artifacts.flags selectedPackages
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
          + lib.concatMapStrings (requirement: requirement.installLicenses) requirements
          + lib.concatMapStrings (check: check.script) stubLayerReuseChecks;
        preBuild = rustPackageDefaults.preBuild + artifactLayers.restore restoredLayers;
        passthru = {
          inherit
            directory
            route
            stubLayerReuseChecks
            testPackages
            ;
          source = sourceAndCargoDeps.src;
          registryArtifacts = registry;
          engineArtifacts = engine;
          unifiedArtifacts = unified;
          frameworkArtifacts = local;
          libraryArtifacts = libraries;
        };
        meta = rustPackageDefaults.meta // lib.optionalAttrs (!library) { mainProgram = program; };
      }
    );
  packagesByName = lib.mapAttrs buildPackage packageDirectories.all;
  packagedCrates = lib.getAttrs (lib.attrNames packageDirectories.packaged) packagesByName;
  libraryCrates = lib.attrValues (lib.getAttrs contract.libraryLayers packagedCrates);
  packages = lib.getAttrs (lib.attrNames packageDirectories.examples) packagesByName // {
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
