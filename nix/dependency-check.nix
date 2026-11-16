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
  rules =
    yankSettings:
    (pkgs.formats.toml { }).generate "deny.toml" {
      graph = {
        targets = linuxTargets ++ lib.attrNames (import ./aosp-targets.nix);
        all-features = true;
      };
      advisories = yankSettings // {
        db-urls = [ advisoryUrl ];
        ignore =
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
      };
      licenses = {
        allow = (lib.importTOML (root + "/about.toml")).accepted;
        include-dev = true;
        private.ignore = true;
      };
    };
in
{
  check = pkgs.stdenv.mkDerivation {
    name = "lavashell-dependencies";
    src = root;
    cargoDeps = sources.vendor (root + "/Cargo.lock");
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
      cargo deny --locked --config ${
        rules { disable-yank-checking = true; }
      } check --deny warnings advisories licenses
    '';
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
    text = ''
      cargo deny --locked --config ${
        rules { disable-yank-checking = false; }
      } check --deny warnings advisories
    '';
    meta.description = "Check the locked crates of the checkout in the working directory for yanked releases";
  };
}
