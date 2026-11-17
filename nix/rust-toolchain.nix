# SPDX-License-Identifier: GPL-3.0-only
{ pkgs, rust-overlay }:
{ targets }:
let
  rust = (rust-overlay.lib.mkRustBin { } pkgs).stable."1.99.0".default.override {
    inherit targets;
  };
in
{
  inherit rust;
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
}
