# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  checkout = import ./checkout-sources.nix { inherit (input) nixpkgs root; };
  inherit (checkout) sources directory;
in
{
  needsEngine = builtins.listToAttrs (
    map (relative: {
      name = relative;
      value = sources.needsEngine (directory relative);
    }) input.directories
  );
  inherit (sources) nativeNames;
}
