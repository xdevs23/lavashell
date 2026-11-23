# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  sources,
  rustPackageDefaults,
  buildInputsOf,
  buildLayers,
}:
let
  inherit (pkgs) lib;
  contract = import ./projection-contract.nix;
  flags =
    names:
    lib.concatMap (name: [
      "--package"
      name
    ]) names;
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  buildScope = import ./cargo-build-scope.nix { inherit pkgs; };
  removeStubbedMembers = import ./stubbed-member-removal.nix { inherit pkgs; };
  clippy = import ./cargo-clippy.nix { inherit pkgs; };
  checkStubLayerReuse = import ./stub-layer-reuse-check.nix { inherit lib buildLayers; };
  selectedPackages =
    directory:
    [ (sources.packageName directory) ]
    ++ lib.filter (
      companion: builtins.elem companion (sources.packageNames directory)
    ) contract.companions;
  packagesTestedThrough =
    layers: lib.concatMap (layer: layer.testPackages) (artifactLayers.stack layers);
  tests = packages: {
    cargoTestFlags = flags packages ++ [ "--frozen" ];
    doCheck = packages != [ ];
  };
  compiledTests =
    packages:
    let
      phase = tests packages;
    in
    phase // { cargoTestFlags = phase.cargoTestFlags ++ [ "--no-run" ]; };
  lint = packages: clippy (flags packages);
  enableFeatures = features: lib.optionalAttrs (features != [ ]) { buildFeatures = features; };
  engineDefaults = rustPackageDefaults // {
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.wayland ];
    preBuild = import ./build-environment.nix {
      inherit pkgs;
      libraries = [ pkgs.wayland ];
    };
    nativeCheckInputs = [ ];
    env = { };
    preCheck = "";
    postInstall = "";
  };
  planCheck =
    directory: selected:
    let
      metadata = sources.metadataFor directory;
      file = builtins.toFile "engine-projection.json" (builtins.toJSON metadata);
    in
    ''
      RUSTC_BOOTSTRAP=1 cargo build --unit-graph -Z unstable-options ${buildScope.target} \
        ${lib.escapeShellArgs (flags selected)} > consumer-cargo-plan.json
      ${lib.getExe' buildLayers "lavashell-plan-check"} engine-units --contract ${./projection-contract.json} \
        --engine ${file} --consumer consumer-cargo-plan.json
    '';
  artifacts =
    {
      pname,
      directory,
      source,
      projection,
      previous ? [ ],
      dummyPackages,
      testedByLayerAbove ? [ ],
      testedByOtherPackages ? [ ],
      configuration,
      validateEngine ? false,
      features ? [ ],
    }:
    let
      names = sources.crateNames projection.crates;
      selected = projection.selectedPackages or (selectedPackages directory);
      testPackages = lib.subtractLists (
        dummyPackages ++ testedByOtherPackages ++ packagesTestedThrough previous
      ) names;
      stubLayer = lib.subtractLists dummyPackages names == [ ];
      stubLayerReuseChecks = lib.optional (!stubLayer) (checkStubLayerReuse previous);
      checkedPackages = if stubLayer then testedByLayerAbove else testPackages;
      testPhase = (if stubLayer then compiledTests else tests) checkedPackages;
    in
    rustPlatform.buildRustPackage (
      configuration
      // testPhase
      // lint checkedPackages
      // enableFeatures features
      // {
        inherit pname;
        src = source;
        cargoDeps = projection.cargoDeps;
        buildInputs = configuration.buildInputs ++ buildInputsOf names;
        cargoBuildFlags = flags selected ++ [ "--frozen" ] ++ rustPackageDefaults.cargoBuildFlags;
        preBuild =
          configuration.preBuild
          + import ./vendor-directory-times.nix
          + artifactLayers.restoreLowerLayers previous
          + lib.optionalString validateEngine (planCheck directory selected);
        postBuild = lib.optionalString stubLayer ''
          ${pkgs.rust.envVars.setEnv} cargo check -j "$NIX_BUILD_CORES" ${buildScope.target} \
            ${lib.escapeShellArgs (flags selected)}
        '';
        installPhase = ''
          runHook preInstall
          ${lib.optionalString (dummyPackages != [ ]) (removeStubbedMembers (flags dummyPackages))}
          ${artifactLayers.storeLayerChanges}
          install -Dm444 cargo-artifacts.jsonl "$out/${artifactLayers.buildRecord}"
          runHook postInstall
        '';
        postInstall = lib.concatMapStrings (check: check.script) stubLayerReuseChecks;
        dontFixup = true;
        passthru = {
          inherit
            previous
            testPackages
            stubLayer
            stubLayerReuseChecks
            ;
        };
      }
    );
  stubArtifacts =
    {
      pname,
      directory,
      layer,
      testedByLayerAbove,
      configuration,
      previous ? [ ],
    }:
    artifacts {
      inherit
        pname
        directory
        previous
        testedByLayerAbove
        configuration
        ;
      inherit (layer) projection source;
      dummyPackages = sources.crateNames layer.projection.crates;
    };
in
{
  registry =
    directory:
    let
      layer = sources.stubLayers.registry directory;
    in
    stubArtifacts {
      pname = "lavashell-registry-artifacts";
      inherit directory layer;
      inherit (layer.projection) testedByLayerAbove;
      configuration = rustPackageDefaults;
    };
  engine =
    directory:
    let
      layer = sources.stubLayers.engine directory;
    in
    stubArtifacts {
      pname = "lavashell-engine-artifacts";
      inherit directory layer;
      inherit (layer.projection) testedByLayerAbove;
      configuration = engineDefaults;
    };
  unified =
    directory: previous: testedByLayerAbove:
    stubArtifacts {
      pname = "lavashell-unified-artifacts";
      layer = sources.stubLayers.unified directory;
      inherit directory previous testedByLayerAbove;
      configuration = rustPackageDefaults;
    };
  framework =
    directory: previous:
    {
      example ? true,
      validateEngine ? false,
      features ? [ ],
      testedByOtherPackages ? [ ],
    }:
    let
      root = sources.packageName directory;
    in
    artifacts {
      pname = "lavashell-framework-artifacts";
      inherit
        directory
        previous
        validateEngine
        features
        testedByOtherPackages
        ;
      projection = sources.project directory;
      source = if example then sources.frameworkSource directory else sources.sourceFor directory;
      dummyPackages = lib.optional example root;
      configuration = rustPackageDefaults;
    };
  inherit
    enableFeatures
    flags
    lint
    packagesTestedThrough
    planCheck
    selectedPackages
    tests
    ;
}
