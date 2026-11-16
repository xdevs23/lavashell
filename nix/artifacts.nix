# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  sources,
  rustPackageDefaults,
  requirementsOf,
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
  planVerifier = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./verify_cargo_plan.py
      ./cargo_plan.py
      ./cargo_models.py
      ./document_tables.py
      ./projection_contract.py
      ./projection-contract.json
      ./verification_errors.py
    ];
  };
  planCheck =
    directory: selected:
    let
      metadata = sources.metadataFor directory;
      file = builtins.toFile "engine-projection.json" (builtins.toJSON metadata);
    in
    ''
      RUSTC_BOOTSTRAP=1 cargo build --unit-graph -Z unstable-options --offline --frozen \
        --profile ${contract.profile} --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} \
        ${lib.escapeShellArgs (flags selected)} > consumer-cargo-plan.json
      ${pkgs.python3}/bin/python3 ${planVerifier}/verify_cargo_plan.py \
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
      configuration,
      withTests,
      validateEngine ? false,
    }:
    let
      names = sources.crateNames projection.crates;
      root = sources.packageName directory;
      selected =
        projection.selectedPackages
          or ([ root ] ++ lib.filter (companion: builtins.elem companion names) contract.companions);
    in
    pkgs.rustPlatform.buildRustPackage (
      configuration
      // {
        inherit pname;
        src = source;
        cargoDeps = sources.vendor projection.lockFile;
        buildInputs =
          configuration.buildInputs
          ++ lib.concatMap (requirement: requirement.buildInputs) (requirementsOf names);
        cargoBuildFlags = flags selected ++ [ "--frozen" ] ++ rustPackageDefaults.cargoBuildFlags;
        cargoTestFlags = flags names ++ [ "--frozen" ];
        doCheck = withTests;
        preBuild =
          configuration.preBuild
          + lib.optionalString withTests (import ./vendor-directory-times.nix)
          + copy previous
          + lib.optionalString validateEngine (planCheck directory selected);
        installPhase = ''
          runHook preInstall
          ${lib.optionalString (dummyPackages != [ ]) ''
            cargo clean --offline --frozen --profile ${contract.profile} \
              --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} ${lib.escapeShellArgs (flags dummyPackages)}
          ''}
          mkdir -p "$out"
          cp -a --reflink=auto target "$out/target"
          install -Dm444 cargo-artifacts.jsonl "$out/share/lavashell/cargo-artifacts.jsonl"
          runHook postInstall
        '';
        postInstall = "";
        dontFixup = true;
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
      withTests = false;
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
      withTests = false;
    };
  framework =
    directory: previous:
    {
      example ? true,
      validateEngine ? false,
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
        ;
      projection = sources.project directory;
      source = if example then sources.frameworkSource directory else sources.sourceFor directory;
      dummyPackages = lib.optional example root;
      configuration = rustPackageDefaults;
      withTests = true;
    };
  inherit
    copy
    flags
    planCheck
    ;
}
