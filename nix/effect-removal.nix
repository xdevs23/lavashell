# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  sources,
  common,
  skia,
}:
let
  directory = root + "/crates/lavashell-renderer";
in
pkgs.rustPlatform.buildRustPackage (
  common
  // {
    pname = "lavashell-effect-removal";
    src = sources.sourceFor directory;
    cargoDeps = sources.vendor (sources.project directory).lockFile;
    auditable = false;
    buildInputs = common.buildInputs ++ [ skia ];
    preBuild = common.preBuild + (import ./vendor-directory-times.nix);
    buildPhase = ''
      runHook preBuild
      cargo run --offline -p lavashell-renderer --no-default-features --example independence-probe
      runHook postBuild
    '';
    doCheck = false;
    installPhase = ''
      touch "$out"
    '';
    postInstall = "";
    dontFixup = true;
  }
)
