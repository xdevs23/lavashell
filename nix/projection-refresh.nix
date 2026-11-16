# SPDX-License-Identifier: GPL-3.0-only
{ pkgs, buildLayers }:
pkgs.writeShellApplication {
  name = "lavashell-refresh-cargo-projections";
  runtimeInputs = with pkgs; [
    cargo
    rustc
  ];
  runtimeEnv.LAVASHELL_NIXPKGS = pkgs.path;
  text = ''
    exec ${pkgs.lib.getExe' buildLayers "lavashell-projection-refresh"} "$@"
  '';
  meta.description = "Refresh the Cargo projections of the checkout in the working directory";
}
