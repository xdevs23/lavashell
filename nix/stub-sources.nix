# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
let
  contract = import ./projection-contract.nix;
in
directory: targets:
lib.concatMapStrings (
  target:
  let
    file = lib.escapeShellArg "${directory}/${target.path}";
  in
  ''
    mkdir -p "$(dirname ${file})"
    printf '%s\n' ${
      lib.escapeShellArg (if target.main then contract.stub.mainSource else contract.stub.librarySource)
    } \
      > ${file}
  ''
) targets
