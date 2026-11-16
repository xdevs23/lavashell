# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  vendor,
}:
let
  workspace = "tools/build-layers";
  directory = root + "/${workspace}";
  testPackage = "lavashell-nix-tests";
  package =
    attributes:
    pkgs.rustPlatform.buildRustPackage (
      {
        version = (pkgs.lib.importTOML (directory + "/Cargo.toml")).workspace.package.version;
        cargoDeps = vendor (directory + "/Cargo.lock");
        meta.license = pkgs.lib.licenses.gpl3Only;
      }
      // attributes
    );
in
{
  tools = package {
    pname = "lavashell-build-layers";
    src = directory;
  };
  nixTests = package {
    pname = testPackage;
    src = root;
    cargoRoot = workspace;
    buildAndTestSubdir = workspace;
    cargoBuildFlags = [
      "--package"
      testPackage
    ];
    cargoTestFlags = [
      "--package"
      testPackage
    ];
    nativeCheckInputs = [ pkgs.nix ];
    env.LAVASHELL_NIXPKGS = "${pkgs.path}";
    preCheck = ''
      export HOME="$TMPDIR" NIX_STORE_DIR="$TMPDIR/store" NIX_STATE_DIR="$TMPDIR/state" NIX_LOG_DIR="$TMPDIR/log"
      nix-store --init
    '';
    dontCargoInstall = true;
    installPhase = ''
      runHook preInstall
      touch "$out"
      runHook postInstall
    '';
  };
}
