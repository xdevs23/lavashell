# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
''--offline --frozen --profile "$cargoBuildType" --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget}''
