# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  cargoLock,
  workspaces,
}:
let
  inherit (pkgs) lib;
  bindings = import ./skia-bindings.nix;
  generated = builtins.fromJSON (builtins.readFile ./skia-source.json);
  source = pkgs.applyPatches {
    name = "skia-source";
    src = pkgs.fetchFromGitHub {
      inherit (generated)
        owner
        repo
        tag
        hash
        ;
    };
    patches = [
      ./skia-exact-float-targets.patch
      ./skia-scissor-clips.patch
      ./skia-host-barrier-stages.patch
      ./skia-cleared-filter-textures.patch
    ];
  };
  bundledExternals = lib.mapAttrs (name: licenses: {
    source = pkgs.fetchgit {
      inherit (generated.externals.${name}) url rev hash;
      fetchSubmodules = false;
    };
    inherit licenses;
  }) (builtins.fromJSON (builtins.readFile ./skia-licenses.json));
  commonGnArguments = {
    cc = "clang";
    cxx = "clang++";
    is_official_build = true;
    is_debug = false;
    skia_use_partition_alloc = false;
    skia_enable_ganesh = false;
    skia_enable_graphite = false;
    skia_use_gl = false;
    skia_use_egl = false;
    skia_use_x11 = false;
    skia_enable_pdf = false;
    skia_enable_svg = false;
    skia_enable_skottie = false;
    skia_use_xps = false;
    skia_use_dng_sdk = false;
    skia_use_libjpeg_turbo_decode = false;
    skia_use_libjpeg_turbo_encode = false;
    skia_use_system_libjpeg_turbo = false;
    skia_use_libwebp_decode = false;
    skia_use_libwebp_encode = false;
    skia_use_system_libpng = false;
    skia_use_system_zlib = false;
    skia_use_expat = true;
    skia_use_system_expat = false;
    skia_use_icu = false;
    skia_use_system_icu = false;
    skia_use_harfbuzz = false;
    skia_use_system_harfbuzz = false;
    skia_use_freetype = true;
    skia_use_freetype_woff2 = false;
  };
  features = {
    embed-icudtl = { };
    no-compile = { };
    ganesh = {
      gnArguments.skia_enable_ganesh = true;
      definingTargets = [ "//:gpu" ];
    };
    jpeg.gnArguments = {
      skia_use_libjpeg_turbo_decode = true;
      skia_use_libjpeg_turbo_encode = true;
    };
    pdf.gnArguments.skia_enable_pdf = true;
    textlayout = {
      gnArguments = {
        skia_enable_skshaper = true;
        skia_enable_skparagraph = true;
        skia_use_icu = true;
        skia_use_harfbuzz = true;
        skia_pdf_subset_harfbuzz = true;
      };
      libraries = [
        "skparagraph"
        "skshaper"
        "skunicode_core"
        "skunicode_icu"
      ];
      definingTargets = [
        "//modules/skshaper:skshaper"
        "//modules/skparagraph:skparagraph"
        "//modules/skunicode:skunicode_core"
        "//modules/skunicode:skunicode_icu"
        "//third_party/icu:icu"
      ];
    };
    vulkan.gnArguments = {
      skia_use_vulkan = true;
      skia_enable_spirv_validation = false;
    };
  };
  featureGnArguments = lib.mergeAttrsList (lib.catAttrs "gnArguments" (lib.attrValues features));
  libraries = [ "skia" ] ++ lib.concatLists (lib.catAttrs "libraries" (lib.attrValues features));
  definingTargets = [
    "//:skia"
  ]
  ++ lib.concatLists (lib.catAttrs "definingTargets" (lib.attrValues features));
  lockChecked = import ./skia-lock-check.nix {
    inherit
      lib
      bindings
      generated
      cargoLock
      workspaces
      ;
    supportedFeatures = lib.attrNames features;
  };
  build =
    {
      name,
      platformGnArguments,
      externals,
      buildInputs ? [ ],
      environment ? { },
    }:
    assert lockChecked;
    let
      externalsDirectory = pkgs.linkFarm "skia-externals" (
        lib.mapAttrs (_: external: external.source) externals
      );
      exports = lib.concatStrings (
        lib.mapAttrsToList (
          variable: value: "\nprintf 'export ${variable}=%q\\n' ${lib.escapeShellArg value}"
        ) environment
      );
    in
    pkgs.clangStdenv.mkDerivation (finalAttrs: {
      inherit name buildInputs;
      __structuredAttrs = true;
      src = source;
      postUnpack = ''
        ln -s ${externalsDirectory} "$sourceRoot/third_party/externals"
      '';
      nativeBuildInputs = [
        pkgs.gn
        pkgs.ninja
        pkgs.python3
        pkgs.jq
      ];
      gnFlags = lib.mapAttrsToList (name: value: "${name}=${builtins.toJSON value}") (
        commonGnArguments // featureGnArguments // platformGnArguments
      );
      ninjaFlags = libraries;
      licenseFiles = {
        LICENSE = "${source}/LICENSE";
      }
      // lib.concatMapAttrs (
        name: external:
        lib.genAttrs' external.licenses (
          file: lib.nameValuePair "${name}/${file}" "${external.source}/${file}"
        )
      ) externals;
      enableParallelBuilding = true;
      installPhase = ''
        runHook preInstall
        for library in ${lib.escapeShellArgs libraries}; do
          install -Dm444 "lib$library.a" "$out/lib/lib$library.a"
        done
        defines=$(
          for target in ${lib.escapeShellArgs definingTargets}; do
            gn desc . "$target" defines --format=json
          done | jq -rs '[.[][].defines[]] | unique | map("-D" + .) | join(" ")'
        )
        mkdir -p "$out/nix-support"
        {
          printf 'export SKIA_SOURCE_DIR=%q\n' ${source}
          printf 'export SKIA_BUILD_DEFINES=%q\n' "$defines"${exports}
        } > "$out/nix-support/skia-environment"
        {
          printf '. %q\n' "$out/nix-support/skia-environment"
          printf 'export SKIA_LIBRARY_SEARCH_PATH=%q\n' "$out/lib"
        } > "$out/nix-support/setup-hook"
        for file in "''${!licenseFiles[@]}"; do
          install -Dm444 "''${licenseFiles[$file]}" "$out/share/licenses/skia/$file"
        done
        runHook postInstall
      '';
      passthru = rec {
        activate = pkgs.writeText "${name}-activate" (
          import ./skia-activate.nix {
            inherit name;
            skia = finalAttrs.finalPackage;
            inherit (builtins) storeDir;
          }
        );
        shellHook = ". ${activate} || exit 1";
        installLicenses = directory: ''
          mkdir -p "$(dirname ${directory})"
          cp -r --no-preserve=mode ${finalAttrs.finalPackage}/share/licenses/skia ${directory}
        '';
      };
      meta.license = [
        lib.licenses.bsd3
        lib.licenses.gpl3Only
      ];
    });
  skiaCpuBySystem.x86_64-linux = "x64";
  system = pkgs.stdenv.hostPlatform.system;
in
{
  linux =
    assert lib.assertMsg (skiaCpuBySystem ? ${system})
      "Skia is built for the Linux hosts ${toString (lib.attrNames skiaCpuBySystem)}, not for ${system}.";
    build {
      name = "skia-${system}";
      externals = removeAttrs bundledExternals [ "freetype" ];
      platformGnArguments = {
        target_os = "linux";
        target_cpu = skiaCpuBySystem.${system};
        skia_use_system_freetype2 = true;
        skia_system_freetype2_include_path = "${lib.getDev pkgs.freetype}/include/freetype2";
      };
      buildInputs = [
        pkgs.fontconfig
        pkgs.freetype
      ];
    };
  aosp =
    {
      sdk,
      ndkPath,
      llvm,
      api,
    }:
    assert lib.assertMsg (lib.isDerivation sdk)
      "Pass the SDK derivation of pkgs.androidenv.composeAndroidPackages { includeNDK = true; } as sdk to the AOSP builds of Skia, and the path of the NDK inside it, starting with a slash, as ndkPath.";
    let
      ndk = "${sdk}${ndkPath}";
    in
    lib.mapAttrs (
      target: targetArguments:
      build {
        name = "skia-${target}";
        externals = bundledExternals;
        platformGnArguments = targetArguments.skia // {
          inherit ndk;
          ndk_api = api;
          skia_use_ndk_images = false;
          skia_enable_fontmgr_android = true;
          skia_use_system_freetype2 = false;
        };
        environment = {
          ANDROID_NDK = ndk;
          SKIA_ANDROID_API_LEVEL = toString api;
          CLANG_PATH = "${llvm}/bin/clang";
          LIBCLANG_PATH = "${llvm}/lib";
        };
      }
    ) (import ./aosp-targets.nix);
}
