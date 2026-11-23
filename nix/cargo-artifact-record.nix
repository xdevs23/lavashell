# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  fileName = "cargo-artifacts.jsonl";
  buildPath = ''"$NIX_BUILD_TOP/${fileName}"'';
  installedPath = "share/lavashell/${fileName}";
in
{
  inherit fileName buildPath installedPath;
  nativeBuildInputs = [ pkgs.jq ];
  messageFormat = "--message-format=json-render-diagnostics";
  run = command: ''
    source ${./cargo-artifact-record.sh}
    recordCargoArtifacts ${buildPath} ${command}
  '';
  install = ''
    install -Dm444 ${buildPath} "$out/${installedPath}"
  '';
}
