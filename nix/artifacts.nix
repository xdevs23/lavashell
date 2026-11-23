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
  types = import ./types.nix;
  flags =
    names:
    lib.concatMap (name: [
      "--package"
      name
    ]) names;
  artifactLayers = import ./artifact-layers.nix { inherit lib; };
  buildScope = import ./cargo-build-scope.nix { inherit pkgs; };
  removeStubbedMembers = import ./stubbed-member-removal.nix { inherit pkgs; };
  artifactRecord = import ./cargo-artifact-record.nix { inherit pkgs; };
  clippy = import ./cargo-clippy.nix { inherit pkgs; };
  checkStubLayerReuse = import ./stub-layer-reuse-check.nix { inherit pkgs buildLayers; };
  packagesTestedThrough =
    layers: lib.concatMap (layer: layer.testPackages) (artifactLayers.stack layers);
  tests = packages: {
    cargoTestFlags = flags packages ++ [
      "--frozen"
      artifactRecord.messageFormat
    ];
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
    nativeBuildInputs = [ pkgs.pkg-config ] ++ artifactRecord.nativeBuildInputs;
    preBuild = "";
    nativeCheckInputs = [ ];
    env = { };
    preCheck = "";
    postInstall = "";
  };
  artifacts =
    {
      pname,
      source,
      projection,
      previous ? [ ],
      layersCheckedForReuse ? previous,
      dummyPackages,
      testedByLayerAbove ? [ ],
      tested,
      configuration,
      features ? [ ],
    }:
    let
      names = sources.crateNames projection.crates;
      inherit (projection) selectedPackages;
      testPackages = lib.subtractLists (dummyPackages ++ packagesTestedThrough previous) (
        lib.intersectLists tested names
      );
      stubLayer = lib.subtractLists dummyPackages names == [ ];
      stubLayerReuseChecks = lib.optional (layersCheckedForReuse != [ ]) (checkStubLayerReuse {
        checkedBuild = if stubLayer then types.checkedBuilds.stubLayer else types.checkedBuilds.consumer;
        lowerLayers = layersCheckedForReuse;
      });
      checkedPackages = if stubLayer then testedByLayerAbove else testPackages;
      testPhase = (if stubLayer then compiledTests else tests) checkedPackages;
    in
    rustPlatform.buildRustPackage (
      configuration
      // testPhase
      // lint checkedPackages
      // enableFeatures features
      // import ./vendored-source.nix {
        src = source;
        inherit (projection) cargoDeps;
      }
      // {
        inherit pname;
        buildInputs = configuration.buildInputs ++ buildInputsOf names;
        cargoBuildFlags = flags selectedPackages ++ [ "--frozen" ] ++ rustPackageDefaults.cargoBuildFlags;
        preBuild = configuration.preBuild + artifactLayers.restoreLowerLayers previous;
        buildPhase =
          configuration.buildPhase
          + lib.optionalString stubLayer (
            artifactRecord.run ''
              ${pkgs.rust.envVars.setEnv} cargo check -j "$NIX_BUILD_CORES" ${buildScope.target} \
                ${lib.escapeShellArgs (flags selectedPackages ++ [ artifactRecord.messageFormat ])}
            ''
          );
        installPhase = ''
          runHook preInstall
          ${lib.optionalString (dummyPackages != [ ]) (removeStubbedMembers (flags dummyPackages))}
          ${artifactLayers.storeLayerChanges}
          ${artifactRecord.install}
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
      layer,
      testedByLayerAbove,
      configuration,
      previous ? [ ],
      layersCheckedForReuse ? [ ],
    }:
    artifacts {
      inherit
        pname
        previous
        layersCheckedForReuse
        testedByLayerAbove
        configuration
        ;
      inherit (layer) projection source;
      dummyPackages = sources.crateNames layer.projection.crates;
      tested = [ ];
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
      inherit layer;
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
      inherit layer;
      inherit (layer.projection) testedByLayerAbove;
      configuration = engineDefaults;
    };
  unified =
    directory:
    { registry, engine }:
    testedByLayerAbove:
    stubArtifacts {
      pname = "lavashell-unified-artifacts";
      layer = sources.stubLayers.unified directory;
      previous = [
        registry
        engine
      ];
      layersCheckedForReuse = [ engine ];
      inherit testedByLayerAbove;
      configuration = rustPackageDefaults;
    };
  framework =
    directory: previous:
    {
      example ? true,
      features ? [ ],
      tested,
    }:
    let
      root = sources.packageName directory;
    in
    artifacts {
      pname = "lavashell-framework-artifacts";
      inherit
        previous
        features
        tested
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
    tests
    ;
}
