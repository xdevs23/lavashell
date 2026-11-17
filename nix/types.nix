# SPDX-License-Identifier: GPL-3.0-only
let
  contract = import ./projection-contract.nix;
  appDeclarations = builtins.fromJSON (builtins.readFile ./app-declarations.json);
  enumOf =
    values:
    builtins.listToAttrs (
      map (value: {
        name = value;
        inherit value;
      }) values
    );
in
{
  hosts = enumOf appDeclarations.hosts;
  launchKinds = enumOf appDeclarations.launchKinds;
  layers = enumOf contract.layers;
  layerRoutes = enumOf [
    "registryOnly"
    "framework"
    "frameworkWithEngine"
  ];
  targetRoles = {
    device = "device";
    emulator = "emulator";
  };
}
