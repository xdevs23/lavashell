# SPDX-License-Identifier: GPL-3.0-only
{ arguments }:
let
  input = builtins.fromJSON arguments;
  vendorDirectory = "cargo-vendor-dir";
  vendored = import (/. + input.nix + "/vendored-source.nix") {
    src.name = "source";
    cargoDeps.name = vendorDirectory;
  };
in
{
  inherit vendorDirectory;
  inherit (vendored) sourceRoot cargoVendorDir;
  unpackedDirectories = map (source: source.name) vendored.srcs;
}
