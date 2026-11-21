# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  root,
  sources,
  rustPackageDefaults,
  skia,
}:
{
  pname,
  crate,
  subcommand,
  arguments,
}:
let
  directory = root + "/crates/${crate}";
in
rustPlatform.buildRustPackage (
  rustPackageDefaults
  // {
    inherit pname;
    src = sources.sourceFor directory;
    cargoDeps = sources.vendor (sources.project directory).lockFile;
    auditable = false;
    buildInputs = rustPackageDefaults.buildInputs ++ [ skia ];
    preBuild = rustPackageDefaults.preBuild + (import ./vendor-directory-times.nix);
    buildPhase = ''
      runHook preBuild
      cargo ${
        pkgs.lib.escapeShellArgs (
          [
            subcommand
            "--offline"
            "-p"
            crate
            "--no-default-features"
          ]
          ++ arguments
        )
      }
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
