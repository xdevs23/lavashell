# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  sources,
  common,
  skia,
}:
let
  inherit (pkgs) lib;
  flags =
    names:
    lib.concatMap (name: [
      "--package"
      name
    ]) names;
  copy =
    previous:
    lib.optionalString (previous != [ ]) ''
      mkdir -p target
      ${lib.concatMapStrings (cache: ''
        cp -a --no-preserve=links --reflink=auto ${cache}/target/. target/
        chmod -R u+w target
      '') previous}
    '';
  engineCommon = common // {
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
      metadata = sources.engineMetadata directory;
      file = builtins.toFile "engine-projection.json" (builtins.toJSON metadata);
    in
    ''
      RUSTC_BOOTSTRAP=1 cargo build --unit-graph -Z unstable-options --offline --frozen \
        --profile packaged --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} \
        ${lib.escapeShellArgs (flags selected)} > consumer-cargo-plan.json
      ${pkgs.python3}/bin/python3 ${./verify-cargo-plan.py} --library ${./cargo-plan.py} \
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
      registry ? false,
      engine ? false,
      validateEngine ? false,
    }:
    let
      names = map (crate: crate.manifest.package.name) projection.crates;
      root = (builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"))).package.name;
      usesRenderer = builtins.elem "lavashell-renderer" names;
      selected =
        projection.selectedPackages or ([ root ] ++ lib.optional usesRenderer "lavashell-renderer");
      configuration = if engine then engineCommon else common;
    in
    pkgs.rustPlatform.buildRustPackage (
      configuration
      // {
        inherit pname;
        src = source;
        cargoDeps = sources.vendor projection.lockFile;
        buildInputs = configuration.buildInputs ++ lib.optional usesRenderer skia;
        cargoBuildFlags = flags selected ++ [ "--frozen" ] ++ common.cargoBuildFlags;
        cargoTestFlags = flags names ++ [ "--frozen" ];
        doCheck = !registry;
        preBuild =
          configuration.preBuild
          + lib.optionalString (!registry) (import ./vendor-directory-times.nix)
          + copy previous
          + lib.optionalString validateEngine (planCheck directory selected);
        installPhase = ''
          runHook preInstall
          ${lib.optionalString (dummyPackages != [ ]) ''
            cargo clean --offline --frozen --profile packaged \
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
      dummyPackages = map (crate: crate.manifest.package.name) (sources.registryProject directory).crates;
      registry = true;
    };
  engine =
    directory:
    artifacts {
      pname = "lavashell-engine-artifacts";
      inherit directory;
      projection = sources.engineProject directory;
      source = sources.engineSource directory;
      dummyPackages = map (crate: crate.manifest.package.name) (sources.engineProject directory).crates;
      registry = true;
      engine = true;
    };
  framework =
    directory: previous:
    {
      example ? true,
      validateEngine ? false,
    }:
    let
      root = (builtins.fromTOML (builtins.readFile (directory + "/Cargo.toml"))).package.name;
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
    };
  inherit
    copy
    flags
    planCheck
    ;
}
