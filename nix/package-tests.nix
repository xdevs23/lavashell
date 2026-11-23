# SPDX-License-Identifier: GPL-3.0-only
{
  lib,
  workspaceCrates,
  packageDirectories,
}:
let
  inherit (workspaceCrates) packageName nativeNames;
  closures = lib.listToAttrs (
    map (
      directory: lib.nameValuePair (packageName directory) (workspaceCrates.packageNames directory)
    ) (lib.attrValues packageDirectories.all)
  );
  testOrder = lib.mapAttrs (root: closure: [
    (lib.length closure)
    root
  ]) closures;
  testedByPackages = lib.mapAttrs (
    root: closure:
    lib.subtractLists (
      nativeNames
      ++ lib.attrNames (removeAttrs closures [ root ])
      ++ lib.concatLists (
        lib.attrValues (lib.filterAttrs (other: _: testOrder.${other} < testOrder.${root}) testedByPackages)
      )
    ) closure
  ) closures;
in
{
  inherit closures;
  testedCrates = testedByPackages // {
    ${packageName workspaceCrates.nativeDirectory} = nativeNames;
  };
}
