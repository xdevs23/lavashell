# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  rustPlatform,
  sources,
  rustPackageDefaults,
  requirementsOf,
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
  copy = import ./cache-copy.nix { inherit lib; };
  buildScope = import ./cargo-build-scope.nix { inherit pkgs; };
  clippy = import ./cargo-clippy.nix { inherit pkgs; };
  selectedPackages =
    directory:
    [ (sources.packageName directory) ]
    ++ lib.filter (
      companion: builtins.elem companion (sources.packageNames directory)
    ) contract.companions;
  packagesTestedThrough =
    layer: layer.testPackages ++ lib.concatMap packagesTestedThrough layer.previous;
  tests = packages: {
    cargoTestFlags = flags packages ++ [ "--frozen" ];
    doCheck = packages != [ ];
  };
  lint = packages: clippy (flags packages);
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
      RUSTC_BOOTSTRAP=1 cargo build --unit-graph -Z unstable-options ${buildScope} \
        ${lib.escapeShellArgs (flags selected)} > consumer-cargo-plan.json
      ${lib.getExe' buildLayers "lavashell-plan-check"} --contract ${./projection-contract.json} \
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
      testedByOtherPackages ? [ ],
      configuration,
      validateEngine ? false,
    }:
    let
      names = sources.crateNames projection.crates;
      selected = projection.selectedPackages or (selectedPackages directory);
      testPackages = lib.subtractLists (
        dummyPackages ++ testedByOtherPackages ++ lib.concatMap packagesTestedThrough previous
      ) names;
      realCrates = lib.subtractLists dummyPackages names;
      stubsLintedToCheckTheirDependencies = lib.optionals (realCrates == [ ]) dummyPackages;
      testPhase = tests testPackages;
    in
    rustPlatform.buildRustPackage (
      configuration
      // testPhase
      // lint (testPackages ++ stubsLintedToCheckTheirDependencies)
      // {
        inherit pname;
        src = source;
        cargoDeps = sources.vendor projection.lockFile;
        buildInputs =
          configuration.buildInputs
          ++ lib.concatMap (requirement: requirement.buildInputs) (requirementsOf names);
        cargoBuildFlags = flags selected ++ [ "--frozen" ] ++ rustPackageDefaults.cargoBuildFlags;
        preBuild =
          configuration.preBuild
          + lib.optionalString testPhase.doCheck (import ./vendor-directory-times.nix)
          + copy previous
          + lib.optionalString validateEngine (planCheck directory selected);
        installPhase = ''
          runHook preInstall
          ${lib.optionalString (dummyPackages != [ ]) ''
            cargo clean ${buildScope} ${lib.escapeShellArgs (flags dummyPackages)}
          ''}
          mkdir -p "$out"
          cp -a --reflink=auto target "$out/target"
          install -Dm444 cargo-artifacts.jsonl "$out/share/lavashell/cargo-artifacts.jsonl"
          runHook postInstall
        '';
        postInstall = "";
        dontFixup = true;
        passthru = { inherit previous testPackages; };
      }
    );
in
{
  registry =
    directory:
    artifacts {
      pname = "lavashell-registry-artifacts";
      inherit directory;
      projection = sources.registryProject directory;
      source = sources.registrySource directory;
      dummyPackages = sources.crateNames (sources.registryProject directory).crates;
      configuration = rustPackageDefaults;
    };
  engine =
    directory:
    artifacts {
      pname = "lavashell-engine-artifacts";
      inherit directory;
      projection = sources.engineProject directory;
      source = sources.engineSource directory;
      dummyPackages = sources.crateNames (sources.engineProject directory).crates;
      configuration = engineDefaults;
    };
  framework =
    directory: previous:
    {
      example ? true,
      validateEngine ? false,
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
        testedByOtherPackages
        ;
      projection = sources.project directory;
      source = if example then sources.frameworkSource directory else sources.sourceFor directory;
      dummyPackages = lib.optional example root;
      configuration = rustPackageDefaults;
    };
  inherit
    copy
    flags
    lint
    packagesTestedThrough
    planCheck
    selectedPackages
    tests
    ;
}
