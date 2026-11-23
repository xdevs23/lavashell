# SPDX-License-Identifier: GPL-3.0-only
{ src, cargoDeps }:
{
  srcs = [
    src
    cargoDeps
  ];
  sourceRoot = src.name;
  cargoVendorDir = "../${cargoDeps.name}";
}
