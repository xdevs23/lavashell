# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  buildScope = import ./cargo-build-scope.nix { inherit pkgs; };
in
packageFlags:
let
  packages = pkgs.lib.escapeShellArgs packageFlags;
in
''
  env --unset=CARGO_BUILD_TARGET cargo clean ${buildScope.host} ${packages}
  cargo clean ${buildScope.target} ${packages}
''
