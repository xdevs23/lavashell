# SPDX-License-Identifier: GPL-3.0-only
{ nixpkgs, root }:
(import ./examples-tree.nix {
  pkgs = import nixpkgs { };
  root = /. + root;
}).examples.reference
