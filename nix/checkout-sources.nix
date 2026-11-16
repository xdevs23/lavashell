# SPDX-License-Identifier: GPL-3.0-only
{ nixpkgs, root }:
let
  checkout = /. + root;
in
{
  sources = import ./source.nix {
    pkgs = import nixpkgs { };
    root = checkout;
    commits = import ./vendored-commits.nix;
  };
  directory = relative: checkout + "/${relative}";
}
