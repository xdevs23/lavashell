# SPDX-License-Identifier: GPL-3.0-only
let
  patch = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).patch.crates-io.skia-bindings;
  locked =
    builtins.filter (package: package.name == "skia-bindings")
      (builtins.fromTOML (builtins.readFile ../Cargo.lock)).package;
in
assert builtins.length locked == 1;
rec {
  inherit (builtins.head locked) version;
  crates = [
    "skia-bindings"
    "skia-safe"
  ];
  url = patch.git;
  inherit (patch) rev;
  hash = "sha256-U2pXvIddP5z2IUyGOrkgavSIZMdQOXNgMwA6Rzu6y3Q=";
  cargoSource = "git+${url}?rev=${rev}#${rev}";
}
