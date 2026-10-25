# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root ? ../.,
  skia ?
    (import ./skia.nix {
      inherit pkgs;
      cargoLock = root + "/Cargo.lock";
      workspaces = [ root ];
    }).linux,
}:
let
  inherit (pkgs) lib;
  runnable = lib.filterAttrs (
    name: type:
    type == "directory"
    && builtins.pathExists (root + "/examples/${name}/Cargo.toml")
    && builtins.pathExists (root + "/examples/${name}/src/main.rs")
  ) (builtins.readDir (root + "/examples"));
  directories = lib.mapAttrs (name: _: root + "/examples/${name}") runnable // {
    wayland-input = root + "/tools/wayland-input";
  };
  launches = {
    wayland-input = {
      kind = "helper";
      arguments = [ "--keyboard" ];
      ready = "ready";
    };
  };
  assets = import ./assets.nix { inherit pkgs; };
  sources = import ./source.nix { inherit pkgs root; };
  notices = import ./notices.nix { inherit pkgs sources root; };
  common = import ./rust-package.nix { inherit pkgs; };
  artifacts = import ./artifacts.nix {
    inherit
      pkgs
      sources
      common
      skia
      ;
  };
  nativeDirectory = root + "/examples/support";
  nativePackages = sources.packageNames nativeDirectory;
  registry = artifacts.registry nativeDirectory;
  framework = artifacts.framework nativeDirectory [ registry ] { example = false; };
  package =
    name: directory:
    let
      program = (builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"))).package.name;
      library = !builtins.pathExists (directory + "/src/main.rs");
      dependencies = lib.remove program (sources.packageNames directory);
      native = !(sources.needsEngine directory);
      engine = if native then null else artifacts.engine directory;
      usesFramework = lib.any (dependency: builtins.elem dependency nativePackages) dependencies;
      usesRenderer = builtins.elem "lavashell-renderer" dependencies;
      usesWayland = builtins.elem "lavashell-wayland" dependencies;
      usesLucide = builtins.elem "lavashell-lucide" dependencies;
      local =
        if native then
          (if usesFramework then framework else artifacts.registry directory)
        else
          artifacts.framework directory (lib.optional usesFramework framework ++ [ engine ]) {
            validateEngine = true;
          };
      selected = [ program ] ++ lib.optional usesRenderer "lavashell-renderer";
    in
    pkgs.rustPlatform.buildRustPackage (
      common
      // {
        pname = "lavashell-${name}";
        src = sources.sourceFor directory;
        cargoDeps = sources.vendor (sources.project directory).lockFile;
        buildInputs = common.buildInputs ++ lib.optional usesRenderer skia;
        cargoBuildFlags =
          artifacts.flags selected
          ++ [ "--frozen" ]
          ++ lib.optionals (!library) [
            "--bin"
            program
          ]
          ++ common.cargoBuildFlags;
        cargoTestFlags = artifacts.flags selected ++ [ "--frozen" ];
        postInstall =
          common.postInstall
          + ''
            install -Dm444 ${notices { inherit directory; }} "$out/share/licenses/lavashell/rust-crates.txt"
          ''
          + lib.optionalString usesLucide ''
            install -Dm444 -t "$out/share/licenses/lucide-static" ${assets.lucide}/LICENSE
          ''
          + lib.optionalString usesRenderer (skia.installLicenses ''"$out/share/licenses/skia"'')
          + lib.optionalString usesWayland ''
            install -Dm444 -t "$out/share/licenses/hyprland-protocols" ${pkgs.hyprland-protocols.src}/LICENSE \
              ${pkgs.hyprland-protocols}/share/hyprland-protocols/protocols/hyprland-focus-grab-v1.xml
          '';
        preBuild =
          common.preBuild
          + (import ./vendor-directory-times.nix)
          + artifacts.copy [ local ]
          + lib.optionalString (!native) (artifacts.planCheck directory selected);
        passthru = {
          inherit directory;
          registryArtifacts = registry;
          engineArtifacts = engine;
          frameworkArtifacts = local;
        };
        meta = common.meta // lib.optionalAttrs (!library) { mainProgram = program; };
      }
    );
  packages = lib.mapAttrs package directories;
  sourceRules = package "source-rules" (root + "/tools/source-rules");
  examples = pkgs.symlinkJoin {
    name = "lavashell-examples-0.1.0";
    paths = lib.attrValues packages;
    meta = common.meta;
  };
in
{
  packages = packages // {
    inherit examples;
  };
  apps = lib.mapAttrs (name: value: {
    type = "app";
    program = lib.getExe value;
    meta.description = value.meta.description;
    meta.lavashellHost = "wayland";
    meta.lavashellLaunch = launches.${name} or { kind = "window"; };
  }) packages;
  checks =
    let
      toolPython = pkgs.python3.withPackages (python: [ python.psutil ]);
    in
    {
      inherit registry framework examples;
      bevy = package "bevy" (root + "/crates/lavashell-bevy");
      effect-removal = import ./effect-removal.nix {
        inherit
          pkgs
          root
          sources
          common
          skia
          ;
      };
      source-limits =
        pkgs.runCommand "lavashell-source-limits"
          {
            nativeBuildInputs = [
              toolPython
              pkgs.ruff
              pkgs.cargo
              sourceRules
            ];
          }
          ''
            cd ${root}
            python3 tools/check-source-limits.py
            python3 tools/crate_rules.py
            lavashell-source-rules
            python3 tools/check-copy.py
            python3 tools/fuzz_workspace.py
            ruff check --no-cache --config tools/ruff.toml tools nix crates/lavashell-protocol/fuzz
            touch "$out"
          '';
      python-tests =
        pkgs.runCommand "lavashell-python-tests"
          {
            nativeBuildInputs = [ toolPython ];
            env.PYTHONDONTWRITEBYTECODE = "1";
          }
          ''
            cd ${root}
            for test in test-cargo-reuse test-dependency-workspace test-layered-progress; do
              python3 "nix/$test.py"
            done
            python3 -m unittest discover -s tools -p 'test_*.py'
            python3 -m unittest discover -s crates/lavashell-protocol/fuzz -p 'test_*.py'
            touch "$out"
          '';
    };
}
