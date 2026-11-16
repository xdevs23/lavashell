# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  advisoryDatabase,
  root,
}:
let
  inherit (pkgs) lib;
  sources = import ./source.nix {
    inherit pkgs root;
    commits = import ./vendored-commits.nix;
  };
  linuxTargets = map (system: (lib.systems.elaborate system).rust.rustcTarget) [
    "x86_64-linux"
    "aarch64-linux"
  ];
  advisoryUrl = "https://github.com/rustsec/advisory-db";
  advisoryRepository =
    pkgs.runCommand "advisory-db.git"
      {
        nativeBuildInputs = [ pkgs.git ];
        env = rec {
          GIT_AUTHOR_NAME = "advisory-db";
          GIT_AUTHOR_EMAIL = GIT_AUTHOR_NAME;
          GIT_AUTHOR_DATE = "@${toString advisoryDatabase.lastModified}";
          GIT_COMMITTER_NAME = GIT_AUTHOR_NAME;
          GIT_COMMITTER_EMAIL = GIT_AUTHOR_NAME;
          GIT_COMMITTER_DATE = GIT_AUTHOR_DATE;
        };
      }
      ''
        cp -r --no-preserve=mode ${advisoryDatabase} "$out"
        git -C "$out" init --quiet --initial-branch=main
        git -C "$out" add --all
        git -C "$out" commit --quiet --message=${advisoryDatabase.rev}
      '';
  zbusAdvisories =
    map
      (id: {
        inherit id;
        reason = "zbus 6 replaces the crate, but atspi 0.30.0 and accesskit_unix 0.24.1, their newest releases, require zbus 5";
      })
      [
        # https://rustsec.org/advisories/RUSTSEC-2026-0370.html
        "RUSTSEC-2026-0370"
        # https://rustsec.org/advisories/RUSTSEC-2026-0371.html
        "RUSTSEC-2026-0371"
        # https://rustsec.org/advisories/RUSTSEC-2026-0372.html
        "RUSTSEC-2026-0372"
      ];
  workspaces = [
    {
      manifest = "Cargo.toml";
      ignoredAdvisories = zbusAdvisories;
      licenses = { };
    }
    {
      manifest = "tools/build-layers/Cargo.toml";
      ignoredAdvisories = [ ];
      licenses.unused-allowed-license = "allow";
    }
  ];
  rules =
    yankSettings: workspace:
    (pkgs.formats.toml { }).generate "deny.toml" {
      graph = {
        targets = linuxTargets ++ lib.attrNames (import ./aosp-targets.nix);
        all-features = true;
      };
      advisories = yankSettings // {
        db-urls = [ advisoryUrl ];
        ignore = workspace.ignoredAdvisories;
      };
      licenses = workspace.licenses // {
        allow = (lib.importTOML (root + "/about.toml")).accepted;
        include-dev = true;
        private.ignore = true;
      };
    };
  denyCommands =
    yankSettings: checks:
    lib.concatMapStrings (workspace: ''
      cargo deny --locked --manifest-path ${workspace.manifest} --config ${rules yankSettings workspace} \
        check --deny warnings ${checks}
    '') workspaces;
in
{
  check = pkgs.stdenv.mkDerivation {
    name = "lavashell-dependencies";
    src = root;
    cargoDeps = pkgs.symlinkJoin {
      name = "cargo-vendor-dir";
      paths = map (
        workspace: sources.vendor (root + "/${dirOf workspace.manifest}/Cargo.lock")
      ) workspaces;
    };
    nativeBuildInputs = with pkgs; [
      rustPlatform.cargoSetupHook
      cargo
      cargo-deny
      git
      rustc
    ];
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
      export CARGO_HOME="$TMPDIR/cargo"
      export GIT_CONFIG_COUNT=1
      export GIT_CONFIG_KEY_0="url.file://${advisoryRepository}.insteadOf" GIT_CONFIG_VALUE_0="${advisoryUrl}"
    ''
    + denyCommands { disable-yank-checking = true; } "advisories licenses";
    installPhase = ''
      touch "$out"
    '';
  };
  yankedCrates = pkgs.writeShellApplication {
    name = "lavashell-yanked-crates";
    runtimeInputs = with pkgs; [
      cargo
      cargo-deny
      git
      rustc
    ];
    text = denyCommands { disable-yank-checking = false; } "advisories";
    meta.description = "Check the locked crates of the checkout in the working directory for yanked releases";
  };
}
