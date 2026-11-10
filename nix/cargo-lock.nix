# SPDX-License-Identifier: GPL-3.0-only
{ lib, root }:
let
  original = builtins.fromTOML (builtins.readFile (root + "/Cargo.lock"));
  key = package: "${package.name} ${package.version} ${package.source or ""}";
  isRegistry = package: package ? source;
  groupByName = lib.groupBy (package: package.name);
  groupByKey = lib.groupBy key;
  lookup = groups: name: groups.${name} or [ ];
  originalByKey = groupByKey original.package;
  originalRegistryByName = groupByName (lib.filter isRegistry original.package);
  findOriginal =
    package:
    let
      matching = lookup originalByKey (key package);
      named = lookup originalRegistryByName package.name;
    in
    if matching != [ ] then
      builtins.head matching
    else if builtins.length named == 1 then
      builtins.head named
    else
      throw "Refresh the Cargo projection for ${key package}";
  matches =
    packagesByName: reference:
    let
      words = lib.splitString " " reference;
    in
    lib.filter (
      package:
      (builtins.length words < 2 || package.version == builtins.elemAt words 1)
      && (
        builtins.length words < 3
        || builtins.elem (lib.concatStringsSep " " (lib.drop 2 words)) [
          (package.source or "")
          "(${package.source or ""})"
        ]
      )
    ) (lookup packagesByName (builtins.head words));
  # A reference by name alone to a crate with several versions in the current lock file resolves to the version in
  # the baseline lock file. The baseline has only one version of that crate, so its references omit the version.
  resolver =
    packages: seeded:
    let
      packagesByName = groupByName packages;
      seededByName = groupByName seeded;
    in
    reference:
    let
      candidates = matches packagesByName reference;
      kept = matches seededByName reference;
    in
    if builtins.length candidates == 1 then
      builtins.head candidates
    else if builtins.length kept == 1 then
      builtins.head kept
    else
      throw "Cargo lock reference must resolve exactly once: ${reference}";
  project =
    baseline: names: local:
    let
      baselineRegistry = lib.filter isRegistry baseline.package;
      resolveInBaseline = resolver baseline.package [ ];
      rebased = map (
        package:
        (findOriginal package)
        // {
          dependencies = map (lockReference: key (findOriginal (resolveInBaseline lockReference))) (
            package.dependencies or [ ]
          );
        }
      ) baselineRegistry;
      rebasedByKey = groupByKey rebased;
      packages =
        rebased
        ++ local
        ++ lib.filter (
          package: isRegistry package && lookup rebasedByKey (key package) == [ ]
        ) original.package;
      resolve = resolver packages rebased;
      toClosureEntry = package: {
        key = key package;
        inherit package;
        dependencies = map resolve (package.dependencies or [ ]);
      };
      closure = lib.genericClosure {
        startSet = map (name: toClosureEntry (resolve name)) names;
        operator = entry: map toClosureEntry entry.dependencies;
      };
      selectedByName = groupByName (map (entry: entry.package) closure);
      reference =
        package:
        package.name
        +
          lib.optionalString (builtins.length (lookup selectedByName package.name) > 1)
            " ${package.version}";
    in
    {
      inherit (original) version;
      package = lib.sort (a: b: key a < key b) (
        map (
          entry:
          entry.package
          // lib.optionalAttrs (entry.package ? dependencies) {
            dependencies = lib.sort builtins.lessThan (map reference entry.dependencies);
          }
        ) closure
      );
    };
in
{
  projected =
    baseline: names:
    project baseline names (lib.filter (package: !isRegistry package) original.package);
  engine =
    baseline: names:
    project baseline names (lib.filter (package: !isRegistry package) baseline.package);
}
