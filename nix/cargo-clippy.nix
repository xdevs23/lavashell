# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  inherit (pkgs) lib;
  buildScope = import ./cargo-build-scope.nix { inherit pkgs; };
  artifactRecord = import ./cargo-artifact-record.nix { inherit pkgs; };
  denyLintsLeftAtWarn = "--deny warnings";
in
selection: {
  cargoClippyFlags = selection ++ [ artifactRecord.messageFormat ];
  preInstallPhases = lib.optional (selection != [ ]) "clippyPhase";
  clippyPhase = ''
    pushd "''${buildAndTestSubdir:-.}"
    ${artifactRecord.run ''
      ${pkgs.rust.envVars.setEnv} cargo clippy -j "$NIX_BUILD_CORES" ${buildScope.target} \
        $cargoClippyFlags --all-targets -- ${denyLintsLeftAtWarn}
    ''}
    popd
  '';
}
