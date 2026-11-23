# SPDX-License-Identifier: GPL-3.0-only
{
  metadata,
  target,
  layer,
  seedProject,
}:
let
  stored = (metadata.seedHashes.${target} or { }).${layer} or null;
in
if stored == null then
  throw "No seed hash is stored for the ${layer} layer on ${target}. Run `nix run .#refresh-cargo-projections` and commit the refreshed projections."
else if stored != seedProject.seedHash then
  throw "The ${layer} seed workspace changed after the projections were last refreshed for ${target}. Run `nix run .#refresh-cargo-projections` and commit the refreshed projections."
else
  seedProject
