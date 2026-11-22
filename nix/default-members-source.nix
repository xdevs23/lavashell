# SPDX-License-Identifier: GPL-3.0-only
{ lib, workspaceCrates }:
{
  root,
  workspace,
  pathDependencies,
}:
let
  inherit (lib) fileset;
  directory = root + "/${workspace}";
  inherit ((lib.importTOML (directory + "/Cargo.toml")).workspace) members default-members;
  targets = import ./cargo-targets.nix { inherit lib; };
  writeStubSources = import ./stub-sources.nix { inherit lib; };
  memberDirectory = member: directory + "/${member}";
  crateOf = member: workspaceCrates.crateAt (memberDirectory member);
  memberByPackage = lib.listToAttrs (
    map (member: lib.nameValuePair (crateOf member).manifest.package.name member) members
  );
  memberDependencies =
    member:
    lib.concatMap (
      table:
      lib.concatLists (
        lib.mapAttrsToList (
          name: dependency:
          lib.optional (memberByPackage ? ${dependency.package or name}) {
            key = memberByPackage.${dependency.package or name};
          }
        ) table
      )
    ) (workspaceCrates.dependencyTables (crateOf member).manifest);
  builtMembers = map (member: member.key) (
    lib.genericClosure {
      startSet = map (member: { key = member; }) default-members;
      operator = member: memberDependencies member.key;
    }
  );
  unbuiltMembers = lib.subtractLists builtMembers members;
  unbuiltDirectories = map memberDirectory unbuiltMembers;
  builtFiles = fileset.difference (fileset.unions ([ directory ] ++ pathDependencies)) (
    fileset.unions unbuiltDirectories
  );
in
{
  src = fileset.toSource {
    inherit root;
    fileset = fileset.unions (
      [ builtFiles ] ++ map (member: member + "/Cargo.toml") unbuiltDirectories
    );
  };
  writeUnbuiltStubs = lib.concatMapStrings (
    member: writeStubSources "${workspace}/${member}" (targets (crateOf member))
  ) unbuiltMembers;
}
