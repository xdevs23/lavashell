# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  root,
  toml,
  workspaceCrates,
  compileManifest,
  targets,
}:
let
  inherit (pkgs) lib;
  inherit (workspaceCrates) relative;
  contract = import ./projection-contract.nix;
in
projectedWorkspace: dummyMembers:
let
  real = lib.subtractLists dummyMembers projectedWorkspace.members;
  sourceFiles =
    lib.optionals (real != [ ]) contract.sourceFiles.everyPackage
    ++ lib.concatMap (member: contract.sourceFiles.byMember.${member} or [ ]) real;
  src = lib.cleanSourceWith {
    name = "source";
    src = root;
    filter =
      path: type:
      let
        name = relative path;
      in
      lib.cleanSourceFilter path type
      && builtins.baseNameOf path != "node_modules"
      && (
        path == toString root
        || lib.any (kept: lib.hasPrefix "${name}/" kept || kept == name || lib.hasPrefix "${kept}/" name) (
          real ++ sourceFiles
        )
      );
  };
  manifests = lib.concatMapStrings (
    crate:
    let
      member = relative crate.key;
      contents = toml.manifest (
        if builtins.elem member dummyMembers then compileManifest crate.manifest else crate.manifest
      );
      file = builtins.toFile "Cargo.toml" contents;
    in
    ''
      mkdir -p "$out/${member}"
      cp ${file} "$out/${member}/Cargo.toml"
    ''
  ) projectedWorkspace.crates;
  stubs = lib.concatMapStrings (
    crate:
    lib.optionalString (builtins.elem (relative crate.key) dummyMembers) (
      lib.concatMapStrings (target: ''
        mkdir -p "$out/$(dirname ${lib.escapeShellArg "${relative crate.key}/${target.path}"})"
        printf '%s\n' ${
          lib.escapeShellArg (if target.main then contract.stub.mainSource else contract.stub.librarySource)
        } \
          > "$out/${relative crate.key}/${target.path}"
      '') (targets crate)
    )
  ) projectedWorkspace.crates;
  manifest = builtins.toFile "Cargo.toml" (toml.manifest projectedWorkspace.manifest);
in
pkgs.runCommand "source" { inherit src; } ''
  mkdir -p "$out"
  cp -R "$src/." "$out/"
  chmod -R u+w "$out"
  cp ${manifest} "$out/Cargo.toml"
  cp ${projectedWorkspace.lockFile} "$out/Cargo.lock"
  ${manifests}
  ${stubs}
''
