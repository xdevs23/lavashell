# SPDX-License-Identifier: GPL-3.0-only
{ pkgs, libraries }:
let
  paths = pkgs.lib.concatMapStringsSep ":" (
    library: "${pkgs.lib.getDev library}/lib/pkgconfig:${pkgs.lib.getDev library}/share/pkgconfig"
  ) libraries;
in
''
  find . -path ./target -prune -o -type f -exec touch -h -r "$cargoDeps" {} +
  find "$NIX_BUILD_TOP/$(stripHash "$cargoDeps")" -type f -exec touch -h -r "$cargoDeps" {} +

  cat > "$NIX_BUILD_TOP/lavashell-pkg-config" <<'PKG_CONFIG_WRAPPER'
  #!${pkgs.runtimeShell}
  export PKG_CONFIG_PATH=${pkgs.lib.escapeShellArg paths}
  export PKG_CONFIG_LIBDIR=${pkgs.lib.escapeShellArg paths}
  exec ${pkgs.pkg-config-unwrapped}/bin/pkg-config "$@"
  PKG_CONFIG_WRAPPER
  chmod +x "$NIX_BUILD_TOP/lavashell-pkg-config"
  export PKG_CONFIG="$NIX_BUILD_TOP/lavashell-pkg-config"
  export HOST_PKG_CONFIG="$PKG_CONFIG" TARGET_PKG_CONFIG="$PKG_CONFIG"
  export PKG_CONFIG_PATH="" HOST_PKG_CONFIG_PATH="" TARGET_PKG_CONFIG_PATH=""
  export PKG_CONFIG_LIBDIR="" HOST_PKG_CONFIG_LIBDIR="" TARGET_PKG_CONFIG_LIBDIR=""
''
