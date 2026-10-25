# SPDX-License-Identifier: GPL-3.0-only
{ root, toml }:
{
  metadata,
  seedHash,
  target,
  layer,
}:
let
  entry = (metadata.dependencyPlans.${target} or { }).${layer} or null;
  fail = reason: throw "${reason}; refresh the ${layer} Cargo dependency projection for ${target}";
  plan =
    if entry == null then
      fail "Missing canonical dependency plan"
    else if entry.seedHash != seedHash then
      fail "Stale dependency plan (seed workspace changed)"
    else if
      entry.plan.schema != 1
      || entry.plan.selection.target != target
      || entry.plan.selection.profile != "packaged"
    then
      fail "Unsupported dependency plan schema, target or profile"
    else
      entry.plan;
  crates = map (crate: {
    key = toString (root + "/${crate.member}");
    inherit (crate) manifest;
    stubTargets = crate.targets;
  }) plan.crates;
in
{
  inherit crates;
  members = map (crate: crate.member) plan.crates;
  manifest = plan.manifest;
  lockFile = builtins.toFile "Cargo.lock" (toml.lock plan.lock);
  selectedPackages = plan.selection.packages;
}
