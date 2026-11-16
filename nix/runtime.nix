# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
{
  libraries = with pkgs; [
    fontconfig
    freetype
    wayland
    libxkbcommon
    vulkan-loader
  ];
  environment.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
}
