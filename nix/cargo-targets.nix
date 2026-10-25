# SPDX-License-Identifier: GPL-3.0-only
{ lib }:
crate:
let
  auto = lib.filter (target: builtins.pathExists (builtins.toPath (crate.key + "/${target.path}"))) [
    {
      path = "src/lib.rs";
      main = false;
    }
    {
      path = "src/main.rs";
      main = true;
    }
    {
      path = "build.rs";
      main = true;
    }
  ];
  explicit =
    lib.optional (builtins.isString (crate.manifest.package.build or false)) {
      path = crate.manifest.package.build;
      main = true;
    }
    ++ lib.optional (crate.manifest ? lib) {
      path = crate.manifest.lib.path or "src/lib.rs";
      main = false;
    }
    ++
      lib.concatMap
        (
          kind:
          map (target: {
            path = target.path or "src/${kind}/${target.name}.rs";
            main = true;
          }) (crate.manifest.${kind} or [ ])
        )
        [
          "bin"
          "test"
          "bench"
          "example"
        ];
in
crate.stubTargets or (lib.unique (auto ++ explicit))
