# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rust,
  root,
}:
let
  inherit (pkgs) lib;
in
pkgs.runCommand "lavashell-rustfmt" { nativeBuildInputs = [ rust ]; } ''
  cd ${root}
  ${lib.concatMapStrings (directory: ''
    cargo fmt --all --check --manifest-path ${lib.escapeShellArg "${directory}/Cargo.toml"}
  '') (lib.attrValues (lib.importTOML ./cargo-workspaces.toml).directories)}
  touch "$out"
''
