# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  pkgs = import input.nixpkgs { };
  inherit (pkgs) lib;
  root = /. + input.root;
  tree =
    (import (/. + input.nix + "/examples-tree.nix") (
      import (/. + input.nix + "/sources-and-assets.nix") { inherit pkgs root; }
      // {
        inherit pkgs root;
        inherit (pkgs) rustPlatform;
      }
    )).examples;
  workspaceCrates = import (/. + input.nix + "/workspace-crates.nix") { inherit lib root; };
in
lib.mapAttrs (_: package: {
  source = "${package.source.src}";
  members = map (crate: workspaceCrates.relative crate.key) (
    workspaceCrates.localCrates package.directory
  );
}) tree.packagedCrates
