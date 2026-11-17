# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  original = import input.nixpkgs { };
  renamed =
    package:
    package.overrideAttrs (old: {
      pname = old.pname + "-fixture";
    });
  pkgs =
    if input.variant == "gui-tools" then
      original
      // {
        fontconfig = renamed original.fontconfig;
        freetype = renamed original.freetype;
        clang = renamed original.clang;
      }
    else
      original;
  tree =
    (import (/. + input.nix + "/examples-tree.nix") {
      inherit pkgs;
      root = /. + input.root;
    }).examples;
  inherit (tree) reference;
in
original.lib.getAttrs input.keys {
  registry = tree.checks.registry.drvPath;
  framework = tree.checks.framework.drvPath;
  engine = reference.engineArtifacts.drvPath;
  referenceFramework = reference.frameworkArtifacts.drvPath;
  reference = reference.drvPath;
  native = tree.packages.interactions.drvPath;
  waylandInput = tree.packages.wayland-input.drvPath;
  aospRoundTrip = tree.aospRoundTrip.drvPath;
  sourceRules = tree.sourceRules.drvPath;
  directory = toString reference.directory;
}
