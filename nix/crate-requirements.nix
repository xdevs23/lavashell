# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  skia,
  assets,
}:
let
  inherit (pkgs) lib;
  requirement =
    {
      buildInputs ? [ ],
      installLicenses ? "",
      nativeCheckInputs ? [ ],
    }:
    {
      inherit buildInputs installLicenses nativeCheckInputs;
    };
  requirements = lib.mapAttrs (_: requirement) {
    lavashell-lucide.installLicenses = ''
      install -Dm444 -t "$out/share/licenses/lucide-static" ${assets.lucide}/LICENSE
    '';
    lavashell-renderer = {
      buildInputs = [ skia ];
      installLicenses = skia.installLicenses ''"$out/share/licenses/skia"'';
    };
    lavashell-repository-hooks.nativeCheckInputs = [ pkgs.git ];
    lavashell-wayland.installLicenses = ''
      install -Dm444 -t "$out/share/licenses/hyprland-protocols" ${pkgs.hyprland-protocols.src}/LICENSE \
        ${pkgs.hyprland-protocols}/share/hyprland-protocols/protocols/hyprland-focus-grab-v1.xml
    '';
  };
in
crateNames: lib.attrValues (lib.filterAttrs (name: _: builtins.elem name crateNames) requirements)
