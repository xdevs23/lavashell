# SPDX-License-Identifier: GPL-3.0-only
{ lib, root }:
let
  original = builtins.fromTOML (builtins.readFile (root + "/Cargo.lock"));
  key = package: "${package.name} ${package.version} ${package.source or ""}";
  matches =
    packages: reference:
    let
      words = lib.splitString " " reference;
    in
    lib.filter (
      package:
      package.name == builtins.head words
      && (builtins.length words < 2 || package.version == builtins.elemAt words 1)
      && (
        builtins.length words < 3
        || builtins.elem (lib.concatStringsSep " " (lib.drop 2 words)) [
          (package.source or "")
          "(${package.source or ""})"
        ]
      )
    ) packages;
  # A reference by name alone to a crate with several versions in the current lock file resolves to the version in
  # the baseline lock file. The baseline has only one version of that crate, so its references omit the version.
  resolver =
    packages: seeded: reference:
    let
      candidates = matches packages reference;
      kept = matches seeded reference;
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
      baselineRegistry = lib.filter (package: package ? source) baseline.package;
      current =
        package:
        let
          matching = lib.filter (entry: key entry == key package) original.package;
          named = lib.filter (entry: entry.name == package.name && entry ? source) original.package;
        in
        if matching != [ ] then
          builtins.head matching
        else if builtins.length named == 1 then
          builtins.head named
        else
          throw "Refresh the Cargo projection for ${key package}";
      rebased = map (
        package:
        (current package)
        // {
          dependencies = map (reference: key (current (resolver baseline.package [ ] reference))) (
            package.dependencies or [ ]
          );
        }
      ) baselineRegistry;
      packages =
        rebased
        ++ local
        ++ lib.filter (
          package: package ? source && !(lib.any (entry: key entry == key package) rebased)
        ) original.package;
      resolve = resolver packages rebased;
      closure = lib.genericClosure {
        startSet = map (name: {
          key = key (resolve name);
          package = resolve name;
        }) names;
        operator =
          entry:
          map (reference: {
            key = key (resolve reference);
            package = resolve reference;
          }) (entry.package.dependencies or [ ]);
      };
      selected = map (entry: entry.package) closure;
      reference =
        dependency:
        let
          package = resolve dependency;
          duplicates = lib.filter (entry: entry.name == package.name) selected;
        in
        package.name + lib.optionalString (builtins.length duplicates > 1) " ${package.version}";
    in
    {
      inherit (original) version;
      package = lib.sort (a: b: key a < key b) (
        map (
          package:
          package
          // lib.optionalAttrs (package ? dependencies) {
            dependencies = lib.sort builtins.lessThan (map reference package.dependencies);
          }
        ) selected
      );
    };
in
{
  projected =
    baseline: names:
    project baseline names (lib.filter (package: !(package ? source)) original.package);
  engine =
    baseline: names:
    project baseline names (lib.filter (package: !(package ? source)) baseline.package);
}
