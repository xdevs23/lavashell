# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  host = ''--offline --frozen --profile "$cargoBuildType"'';
in
{
  inherit host;
  target = "${host} --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget}";
}
