# SPDX-License-Identifier: GPL-3.0-only
{
  description = "Lavashell native Vulkan and Wayland UI toolkit";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay/e60029353d0c48d216bc4b065168ccd4079c166f";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.advisory-db = {
    url = "github:rustsec/advisory-db";
    flake = false;
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      advisory-db,
    }:
    let
      system = "x86_64-linux";
      root = ./.;
      pkgs = nixpkgs.legacyPackages.${system};
      inherit (pkgs) lib;
      types = import ./nix/types.nix;
      runtime = import ./nix/runtime.nix { inherit pkgs; };
      rustToolchain = import ./nix/rust-toolchain.nix { inherit pkgs rust-overlay; };
      inherit (rustToolchain { targets = [ ]; }) rust rustPlatform;
      toolchain = import ./nix/aosp-toolchain.nix {
        inherit
          nixpkgs
          rustToolchain
          system
          root
          ;
      };
      inherit (import ./nix/sources-and-assets.nix { inherit pkgs root; }) sources assets;
      inherit
        (import ./nix/examples-tree.nix {
          inherit
            pkgs
            root
            rustPlatform
            sources
            assets
            ;
        })
        skia
        examples
        ;
      projectionRefresh = import ./nix/projection-refresh.nix {
        inherit pkgs rustPlatform;
        inherit (examples) buildLayers;
      };
      aospSkia =
        (self.lib.skia {
          inherit (toolchain) pkgs;
          cargoLock = ./Cargo.lock;
          workspaces = [ ./. ];
        }).aosp
          {
            inherit (toolchain) sdk ndkPath;
            llvm = toolchain.llvmRoot;
            api = toolchain.nativeApi;
          };
      aospSkiaBuilds = lib.mapAttrs' (_: build: lib.nameValuePair build.name build) aospSkia;
      nativeTestTools = with pkgs; [
        sway
        dbus
      ];
      shell =
        { packages, environment }:
        pkgs.mkShell (
          assets.environment
          // assets.testEnvironment
          // assets.waylandEnvironment
          // runtime.environment
          // environment
          // {
            packages = [
              rust
            ]
            ++ (with pkgs; [
              clang
              pkg-config
              fontconfig
              wayland
              libxkbcommon
              vulkan-loader
              nixfmt
              nix-prefetch-git
              ruff
              (python3.withPackages (python: [
                python.psutil
                python.tomli-w
              ]))
            ])
            ++ packages;
            LD_LIBRARY_PATH = lib.makeLibraryPath runtime.libraries;
            LAVASHELL_NIXPKGS = pkgs.path;
            FONTCONFIG_FILE = "${pkgs.fontconfig.out}/etc/fonts/fonts.conf";
            shellHook = skia.shellHook;
          }
        );
      aosp = import ./nix/aosp.nix {
        inherit
          toolchain
          root
          sources
          assets
          ;
        inherit (examples) buildLayers;
        aospRoundTrip = examples.packagedCrates.aosp-round-trip;
        skia = aospSkia;
      };
      dependencies = import ./nix/dependency-check.nix {
        inherit
          pkgs
          root
          rustPlatform
          ;
        inherit (sources) cargoWorkspaces;
        advisoryDatabase = advisory-db;
      };
    in
    {
      lib.skia = import ./nix/skia.nix;
      lib.skiaBindings = import ./nix/skia-bindings.nix;
      packages.${system} =
        examples.packages
        // aosp.packages
        // aospSkiaBuilds
        // {
          ${skia.name} = skia;
        }
        // {
          default = examples.packages.counter;
          lucide-static = assets.lucide;
        };
      apps.${system} =
        examples.apps
        // aosp.apps
        // {
          default = examples.apps.counter;
          yanked-crates = {
            type = "app";
            program = lib.getExe dependencies.yankedCrates;
            meta.description = dependencies.yankedCrates.meta.description;
            meta.lavashellHost = types.hosts.checkout;
          };
          refresh-cargo-projections = {
            type = "app";
            program = lib.getExe projectionRefresh;
            meta.description = projectionRefresh.meta.description;
            meta.lavashellHost = types.hosts.checkout;
          };
          repository-hooks = {
            type = "app";
            program = lib.getExe examples.packagedCrates.repository-hooks;
            meta.description = "Check a commit or a push against the rules of the repository";
            meta.lavashellHost = types.hosts.checkout;
          };
        };
      checks.${system} =
        examples.checks
        // aosp.packages
        // aospSkiaBuilds
        // {
          dependencies = dependencies.check;
          rustfmt = import ./nix/rustfmt-check.nix { inherit pkgs root rust; };
        };
      devShells.${system} = {
        aosp = aosp.shell;
        default = shell {
          packages = [ ];
          environment = { };
        };
        native = shell {
          packages = nativeTestTools;
          environment = { };
        };
        validation = shell {
          packages = nativeTestTools;
          environment.VK_LAYER_PATH = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
        };
      };
      formatter.${system} = pkgs.nixfmt-tree;
    };
}
