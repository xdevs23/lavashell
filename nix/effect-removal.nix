# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  root,
  sources,
  rustPackageDefaults,
  skia,
}:
let
  directory = root + "/crates/lavashell-renderer";
in
rustPlatform.buildRustPackage (
  rustPackageDefaults
  // {
    pname = "lavashell-effect-removal";
    src = sources.sourceFor directory;
    cargoDeps = sources.vendor (sources.project directory).lockFile;
    auditable = false;
    buildInputs = rustPackageDefaults.buildInputs ++ [ skia ];
    preBuild = rustPackageDefaults.preBuild + (import ./vendor-directory-times.nix);
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
