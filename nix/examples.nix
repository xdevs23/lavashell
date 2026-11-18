# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  skia,
  rustPlatform,
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
  packageRoots = map sources.packageName (
    lib.attrValues directories ++ lib.attrValues packagedDirectories
  );
  launches = {
    wayland-input = {
      kind = types.launchKinds.helper;
      arguments = [ "--keyboard" ];
    };
  };
  assets = import ./assets.nix { inherit pkgs; };
  sources = import ./source.nix {
    inherit pkgs root;
    commits = import ./vendored-commits.nix;
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
    inherit pkgs;
    inherit (sources) version;
  };
  requirementsOf = import ./crate-requirements.nix { inherit pkgs skia assets; };
  layerTools = import ./build-layers.nix {
    inherit pkgs root rustPlatform;
    inherit (sources) vendor;
  };
  buildLayers = layerTools.tools;
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
  packageSkippingTestsOf =
    testedElsewhere: name: directory:
    let
      program = sources.packageName directory;
      library = !builtins.pathExists (directory + "/src/main.rs");
      crates = sources.localCrates directory;
      closure = sources.crateNames crates;
      dependencies = lib.remove program closure;
      route = sources.layerRoute {
        package = program;
        inherit crates;
      };
      withEngine = route == types.layerRoutes.frameworkWithEngine;
      engine = if withEngine then artifacts.engine directory else null;
      requirements = requirementsOf dependencies;
      testedByOtherPackages =
        lib.remove program packageRoots ++ lib.concatMap (other: other.testPackages) testedElsewhere;
      local =
        {
          ${types.layerRoutes.registryOnly} = artifacts.registry directory;
          ${types.layerRoutes.framework} = framework;
          ${types.layerRoutes.frameworkWithEngine} = artifacts.framework directory [ framework engine ] {
            validateEngine = true;
            inherit testedByOtherPackages;
          };
        }
        .${route};
      selected = artifacts.selectedPackages directory;
      testPackages = lib.subtractLists (
        lib.concatMap artifacts.packagesTestedThrough [
          framework
          local
        ]
        ++ testedByOtherPackages
      ) closure;
    in
    rustPlatform.buildRustPackage (
      rustPackageDefaults
      // artifacts.tests testPackages
      // artifacts.lint testPackages
      // {
        pname = "lavashell-${name}";
        src = sources.sourceFor directory;
        cargoDeps = sources.vendor (sources.project directory).lockFile;
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
          + artifacts.copy [ local ]
          + lib.optionalString withEngine (artifacts.planCheck directory selected);
        passthru = {
          inherit directory route testPackages;
          registryArtifacts = registry;
          engineArtifacts = engine;
          frameworkArtifacts = local;
        };
        meta = rustPackageDefaults.meta // lib.optionalAttrs (!library) { mainProgram = program; };
      }
    );
  testedElsewhere = {
    native-harness = [ packagedCrates.aosp-round-trip ];
  };
  packagedCrates = lib.mapAttrs (
    name: packageSkippingTestsOf (testedElsewhere.${name} or [ ]) name
  ) packagedDirectories;
  packages = lib.mapAttrs (packageSkippingTestsOf [ ]) directories // {
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
    in
    packagedCrates
    // {
      inherit registry framework examples;
      nix-tests = layerTools.nixTests;
      effect-removal = import ./effect-removal.nix {
        inherit
          pkgs
          rustPlatform
          root
          sources
          rustPackageDefaults
          skia
          ;
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
