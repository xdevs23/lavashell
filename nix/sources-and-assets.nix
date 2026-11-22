# SPDX-License-Identifier: GPL-3.0-only
{ pkgs, root }:
{
  assets = import ./assets.nix { inherit pkgs; };
  sources = import ./source.nix {
    inherit pkgs root;
    projections = import ./checkout-projections.nix root;
    commits = import ./vendored-commits.nix;
  };
}
