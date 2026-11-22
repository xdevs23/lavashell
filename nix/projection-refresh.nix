# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  buildLayers,
}:
let
  name = "lavashell-refresh-cargo-projections";
in
pkgs.runCommand name
  {
    nativeBuildInputs = [ pkgs.makeWrapper ];
    meta = {
      description = "Refresh the Cargo projections for the checkout in the working directory";
      mainProgram = name;
    };
  }
  ''
    makeWrapper ${pkgs.lib.getExe' buildLayers "lavashell-projection-refresh"} \
      "$out/bin/${name}" \
      --prefix PATH : ${
        pkgs.lib.makeBinPath [
          rustPlatform.rust.cargo
          rustPlatform.rust.rustc
        ]
      } \
      --set LAVASHELL_NIXPKGS ${pkgs.path}
  ''
