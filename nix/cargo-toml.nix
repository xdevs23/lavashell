# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
let
  value =
    item:
    if builtins.isAttrs item then
      "{ "
      + lib.concatStringsSep ", " (
        lib.mapAttrsToList (name: item: "${builtins.toJSON name} = ${value item}") item
      )
      + " }"
    else if builtins.isList item then
      "[ " + lib.concatMapStringsSep ", " value item + " ]"
    else
      builtins.toJSON item;
  fields =
    item:
    lib.concatStrings (
      lib.mapAttrsToList (name: item: "${builtins.toJSON name} = ${value item}\n") item
    );
  section = name: item: "[${builtins.toJSON name}]\n${fields item}\n";
in
{
  manifest =
    data:
    lib.concatStrings (
      lib.mapAttrsToList (
        name: item:
        if builtins.isList item then
          lib.concatMapStrings (entry: "[[${builtins.toJSON name}]]\n${fields entry}\n") item
        else
          section name item
      ) data
    );
  lock =
    data:
    "version = ${toString data.version}\n\n"
    + lib.concatMapStrings (package: "[[package]]\n${fields package}\n") data.package;
}
