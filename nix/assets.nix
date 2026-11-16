# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  inherit (pkgs) lib;
  lock = builtins.fromJSON (builtins.readFile ../examples/package-lock.json);
  dependencies = lib.filterAttrs (name: _: lib.hasPrefix "node_modules/" name) lock.packages;
  install =
    name: package:
    let
      archive = pkgs.fetchurl {
        url = package.resolved;
        hash = package.integrity;
      };
    in
    ''
      mkdir -p "$out/${name}"
      tar -xzf ${archive} --strip-components=1 -C "$out/${name}"
    '';
  fetchStaticNotoSansFace =
    face: hash:
    pkgs.fetchurl {
      url = "https://raw.githubusercontent.com/notofonts/notofonts.github.io/noto-monthly-release-2026.09.01/fonts/NotoSans/unhinted/ttf/NotoSans-${face}.ttf";
      inherit hash;
    };
  staticNotoSansFaces = lib.mapAttrsToList fetchStaticNotoSansFace {
    Regular = "sha256-85YanN4BbUGkh5rs2hR006Nta/VPoORkPeApzCJIsOg=";
    Bold = "sha256-h8sthEcqfWbaZZ7ke2zblVIybowSgkUjHxkbasclKdk=";
  };
in
rec {
  nodeModules = pkgs.runCommand "lavashell-example-assets" { } (
    lib.concatStrings (lib.mapAttrsToList install dependencies)
  );
  lucide = pkgs.fetchzip {
    name = "lucide-static-1.56.0";
    url = "https://registry.npmjs.org/lucide-static/-/lucide-static-1.56.0.tgz";
    hash = "sha256-RF+BW52TOushmSHo3NZbfn1f/wDzWlz3Hr623GM0a0k=";
  };
  environment = {
    LAVASHELL_EXAMPLE_NODE_MODULES = "${nodeModules}/node_modules";
    LAVASHELL_LUCIDE = "${lucide}";
  };
  testEnvironment = {
    LAVASHELL_TEST_FONT = "${pkgs.noto-fonts}/share/fonts/noto/NotoSans.ttf";
    LAVASHELL_TEST_STATIC_FONTS = lib.concatStringsSep ":" staticNotoSansFaces;
  };
  waylandEnvironment = {
    LAVASHELL_HYPRLAND_PROTOCOLS = "${pkgs.hyprland-protocols}";
    LAVASHELL_XKB_CONFIG_ROOT = "${pkgs.xkeyboard-config}/share/X11/xkb";
  };
}
