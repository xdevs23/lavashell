# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  root,
  vendor,
}:
let
  workspace =
    (import ./cargo-workspaces.nix {
      inherit (pkgs) lib;
      inherit root;
    }).buildLayers;
  directory = root + "/${workspace}";
  testPackage = "lavashell-nix-tests";
  clippy = import ./cargo-clippy.nix { inherit pkgs; };
  package =
    attributes:
    rustPlatform.buildRustPackage (
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
  nixTests = package (
    clippy [ "--workspace" ]
    // {
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
    }
  );
}
