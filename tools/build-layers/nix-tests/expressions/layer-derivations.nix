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
  root = /. + input.root;
  tree =
    (import (/. + input.nix + "/examples-tree.nix") (
      import (/. + input.nix + "/sources-and-assets.nix") { inherit pkgs root; }
      // {
        inherit pkgs root;
        inherit (original) rustPlatform;
      }
    )).examples;
  inherit (tree) reference;
  types = import (/. + input.nix + "/types.nix");
  packagedCrates = original.lib.filterAttrs (
    _: package: package.directory != reference.directory
  ) tree.packagedCrates;
  namesByRoute = original.lib.groupBy (name: packagedCrates.${name}.route) (
    builtins.attrNames packagedCrates
  );
  cratesWithEngine = map (name: {
    engine = "${name}-engine";
    framework = "${name}-framework";
    package = name;
  }) namesByRoute.${types.layerRoutes.frameworkWithEngine};
in
original.lib.getAttrs input.keys (
  {
    registry = tree.checks.registry.drvPath;
    framework = tree.checks.framework.drvPath;
    engine = reference.engineArtifacts.drvPath;
    referenceFramework = reference.frameworkArtifacts.drvPath;
    reference = reference.drvPath;
    native = tree.packages.interactions.drvPath;
    directory = toString reference.directory;
    packagesByRoute = namesByRoute // {
      ${types.layerRoutes.frameworkWithEngine} = cratesWithEngine;
    };
  }
  // builtins.mapAttrs (_: package: package.drvPath) packagedCrates
  // original.lib.mergeAttrsList (
    map (layers: {
      ${layers.engine} = packagedCrates.${layers.package}.engineArtifacts.drvPath;
      ${layers.framework} = packagedCrates.${layers.package}.frameworkArtifacts.drvPath;
    }) cratesWithEngine
  )
)
