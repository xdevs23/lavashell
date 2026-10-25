# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  tools = import ./rust-tools.nix pkgs ++ [
    pkgs.cargo-fuzz
    pkgs.python3
  ];
in
{
  shell = pkgs.mkShell { packages = tools; };
  runner = pkgs.writeShellApplication {
    name = "lavashell-protocol-fuzz";
    runtimeInputs = tools ++ [ pkgs.stdenv.cc ];
    text = ''
      exec python3 crates/lavashell-protocol/fuzz/run.py "$@"
    '';
    meta.description = "Fuzz the protocol crate of the checkout in the working directory";
  };
}
