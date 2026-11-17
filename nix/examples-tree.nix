# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  rustPlatform,
}:
let
  skia =
    (import ./skia.nix {
      inherit pkgs;
      cargoLock = root + "/Cargo.lock";
      workspaces = [ root ];
    }).linux;
in
{
  inherit skia;
  examples = import ./examples.nix {
    inherit
      pkgs
      root
      skia
      rustPlatform
      ;
  };
}
