# SPDX-License-Identifier: GPL-3.0-only
{
  nixpkgs,
  root,
  projections,
}:
let
  checkout = /. + root;
in
{
  sources = import ./source.nix {
    pkgs = import nixpkgs { };
    root = checkout;
    projections = /. + projections;
    commits = import ./vendored-commits.nix;
  };
  directory = relative: checkout + "/${relative}";
}
