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
      pkgs = nixpkgs.legacyPackages.${system};
      inherit (pkgs) lib;
      toolchain = import ./nix/android-toolchain.nix { inherit nixpkgs rust-overlay system; };
      skia =
        (self.lib.skia {
          inherit pkgs;
          cargoLock = ./Cargo.lock;
          workspaces = [ ./. ];
        }).linux;
      androidSkia =
        (self.lib.skia {
          inherit (toolchain) pkgs;
          cargoLock = ./Cargo.lock;
          workspaces = [ ./. ];
        }).android
          {
            ndk = toolchain.ndkRoot;
            api = toolchain.nativeApi;
          };
      examples = import ./nix/examples.nix { inherit pkgs skia; };
      assets = import ./nix/assets.nix { inherit pkgs; };
      shell =
        {
          nativeTestTools ? false,
          vulkanValidation ? false,
        }:
        pkgs.mkShell (
          assets.environment
          // assets.testEnvironment
          // assets.waylandEnvironment
          // {
            packages =
              (with pkgs; [
                cargo
                rustc
                rustfmt
                clippy
                clang
                pkg-config
                fontconfig
                wayland
                libxkbcommon
                vulkan-loader
                nixfmt
                ruff
                (python3.withPackages (python: [
                  python.inotify-simple
                  python.pillow
                  python.psutil
                ]))
              ])
              ++ lib.optionals nativeTestTools (
                with pkgs;
                [
                  sway
                  grim
                  dbus
                ]
              );
            VK_LAYER_PATH = lib.optionalString vulkanValidation (
              "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d"
            );
            LD_LIBRARY_PATH = lib.makeLibraryPath (
              with pkgs;
              [
                fontconfig
                freetype
                wayland
                libxkbcommon
                vulkan-loader
              ]
            );
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            FONTCONFIG_FILE = "${pkgs.fontconfig.out}/etc/fonts/fonts.conf";
            shellHook = skia.shellHook;
          }
        );
      android = import ./nix/android.nix {
        inherit toolchain;
        skia = androidSkia;
      };
      dependencies = import ./nix/dependency-check.nix {
        inherit pkgs;
        advisoryDatabase = advisory-db;
      };
    in
    {
      lib.skia =
        {
          pkgs,
          cargoLock,
          workspaces,
        }:
        import ./nix/skia.nix { inherit pkgs cargoLock workspaces; };
      lib.skiaBindings = import ./nix/skia-bindings.nix;
      packages.${system} =
        examples.packages
        // android.packages
        // lib.mapAttrs' (_: build: lib.nameValuePair build.name build) (androidSkia // { linux = skia; })
        // {
          default = examples.packages.counter;
          lucide-static = assets.lucide;
        };
      apps.${system} =
        examples.apps
        // android.apps
        // {
          default = examples.apps.counter;
          yanked-crates = {
            type = "app";
            program = lib.getExe dependencies.yankedCrates;
            meta.description = dependencies.yankedCrates.meta.description;
            meta.lavashellHost = "checkout";
          };
        };
      checks.${system} = examples.checks // {
        dependencies = dependencies.check;
      };
      devShells.${system} = {
        android = android.shell;
        default = shell { };
        native = shell { nativeTestTools = true; };
        validation = shell {
          nativeTestTools = true;
          vulkanValidation = true;
        };
      };
      formatter.${system} = pkgs.nixfmt-tree;
    };
}
