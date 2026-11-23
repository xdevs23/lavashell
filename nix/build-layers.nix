# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  root,
  cargoWorkspace,
}:
let
  cargoWorkspaces = pkgs.lib.importTOML ./cargo-workspaces.toml;
  workspace = cargoWorkspace.directory;
  directory = root + "/${workspace}";
  manifest = pkgs.lib.importTOML (directory + "/Cargo.toml");
  pathDependencies = map (dependency: directory + "/${dependency.path}") (
    builtins.filter (dependency: dependency ? path) (
      builtins.attrValues manifest.workspace.dependencies
    )
  );
  defaultMembersSource = import ./default-members-source.nix {
    inherit (pkgs) lib;
    workspaceCrates = import ./workspace-crates.nix {
      inherit (pkgs) lib;
      inherit root;
    };
  } { inherit root workspace pathDependencies; };
  inheritedRootManifest = (pkgs.formats.toml { }).generate "Cargo.toml" {
    workspace =
      pkgs.lib.getAttrs cargoWorkspaces.inherited-tables
        (pkgs.lib.importTOML (root + "/Cargo.toml")).workspace;
  };
  testPackage = "lavashell-nix-tests";
  clippy = import ./cargo-clippy.nix { inherit pkgs; };
  artifactRecord = import ./cargo-artifact-record.nix { inherit pkgs; };
  package =
    attributes:
    rustPlatform.buildRustPackage (
      {
        version = manifest.workspace.package.version;
        inherit (cargoWorkspace) cargoDeps;
        meta.license = pkgs.lib.licenses.gpl3Only;
      }
      // attributes
    );
in
{
  tools = package {
    pname = "lavashell-build-layers";
    inherit (defaultMembersSource) src;
    postPatch = ''
      cp ${inheritedRootManifest} Cargo.toml
      ${defaultMembersSource.writeUnbuiltStubs}
    '';
    cargoRoot = workspace;
    buildAndTestSubdir = workspace;
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
      nativeBuildInputs = artifactRecord.nativeBuildInputs;
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
