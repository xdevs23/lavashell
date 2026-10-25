# SPDX-License-Identifier: GPL-3.0-only
{ lib, tree }:
lib.findFirst (candidate: (candidate.engineArtifacts or null) != null)
  (throw "no package or check builds on the engine layer")
  (lib.attrValues (tree.packages // tree.checks))
