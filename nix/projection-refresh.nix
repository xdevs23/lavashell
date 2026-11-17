# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  buildLayers,
}:
pkgs.writeShellApplication {
  name = "lavashell-refresh-cargo-projections";
  runtimeInputs = [
    rustPlatform.rust.cargo
    rustPlatform.rust.rustc
  ];
  runtimeEnv.LAVASHELL_NIXPKGS = pkgs.path;
  text = ''
    exec ${pkgs.lib.getExe' buildLayers "lavashell-projection-refresh"} "$@"
  '';
  meta.description = "Refresh the Cargo projections of the checkout in the working directory";
}
