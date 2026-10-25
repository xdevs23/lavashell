# SPDX-License-Identifier: GPL-3.0-only
# Each entry of `commits` has the `cargoSource` by which a lock file refers to a git commit, and the `hash` of the
# tree of that commit without submodules.
{ pkgs, commits }:
let
  inherit (pkgs) lib;
  hashes = lib.listToAttrs (map (commit: lib.nameValuePair commit.cargoSource commit.hash) commits);
  importCargoLock = pkgs.rustPlatform.importCargoLock.override {
    fetchgit = arguments: pkgs.fetchgit (arguments // { fetchSubmodules = false; });
  };
in
lockFile:
importCargoLock {
  inherit lockFile;
  outputHashes = lib.listToAttrs (
    map
      (
        package:
        lib.nameValuePair "${package.name}-${package.version}" (
          hashes.${package.source} or (throw ''
            ${toString lockFile} takes ${package.name} from ${package.source}, a commit without a hash in Nix.
            Pass that commit with the hash of its tree to nix/cargo-vendor.nix.'')
        )
      )
      (
        lib.filter (package: lib.hasPrefix "git+" (package.source or "")) (lib.importTOML lockFile).package
      )
  );
}
