# SPDX-License-Identifier: GPL-3.0-only
# Cargo builds skia-bindings and skia-safe from this commit of a rust-skia fork, set by the `[patch.crates-io]` tables
# of the workspace manifest.
let
  patch = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).patch.crates-io.skia-bindings;
in
rec {
  version = "0.153.3";
  crates = [
    "skia-bindings"
    "skia-safe"
  ];
  url = patch.git;
  inherit (patch) rev;
  hash = "sha256-ysuv1X/LriNEItu9FBiDITaGwKYl0urgUOcIDPQTld4=";
  cargoSource = "git+${url}?rev=${rev}#${rev}";
}
