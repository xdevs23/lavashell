# SPDX-License-Identifier: GPL-3.0-only
# Builds the Skia libraries linked by skia-bindings. Evaluation fails unless `cargoLock` locks skia-safe and
# skia-bindings to the version and the rust-skia commit set in skia-bindings.nix, and unless the crates of
# `workspaces` enable exactly the skia-safe features provided by these builds.
{
  pkgs,
  cargoLock,
  workspaces,
}:
let
  inherit (pkgs) lib;
  bindings = import ./skia-bindings.nix;
  source = pkgs.applyPatches {
    name = "skia-source";
    src = pkgs.fetchFromGitHub {
      owner = "rust-skia";
      repo = "skia";
      tag = "m153-0.101.2";
      hash = "sha256-xKCkvQoTAMlLvKdVzzSxTST6kER4kRUIDjvTRZ9z72o=";
    };
    patches = [
      # A draw and a clear store a solid color in a float target the same way, rounded to the precision of the target.
      ./skia-exact-float-targets.patch
      # A clip on whole device pixels only limits draws to those pixels and changes no other clip or draw.
      ./skia-scissor-clips.patch
      # A pipeline barrier on host access includes the host stage.
      ./skia-host-barrier-stages.patch
      # The intermediate textures created for image filters start cleared, so a filter never samples unwritten memory.
      ./skia-cleared-filter-textures.patch
    ];
  };
  bundledExternals =
    lib.mapAttrs
      (_: external: {
        source = pkgs.fetchgit {
          inherit (external) url rev hash;
          fetchSubmodules = false;
        };
        inherit (external) licenses;
      })
      {
        expat = {
          url = "https://chromium.googlesource.com/external/github.com/libexpat/libexpat.git";
          rev = "6154446fccefbf3ca644894f598969113b0c7bcd";
          hash = "sha256-ifMDdEWu9H8iJ/VSKuXlisJKx1s1F8ecT74bwXnBWcA=";
          licenses = [ "COPYING" ];
        };
        freetype = {
          url = "https://chromium.googlesource.com/chromium/src/third_party/freetype2.git";
          rev = "264b5fbf5b912b39f98d038bf75d39be0a73f21b";
          hash = "sha256-UeV2TAEDylx72nRVrzIVquzfduHgHWvO2Lw+OTWZrqE=";
          licenses = [
            "LICENSE.TXT"
            "docs/FTL.TXT"
          ];
        };
        harfbuzz = {
          url = "https://chromium.googlesource.com/external/github.com/harfbuzz/harfbuzz.git";
          rev = "9cb1fee51069b206effb4736e443b038d230789d";
          hash = "sha256-EYItQuWvB4q2Q2iJfMyEtX9MTm3rEFlcwZ+NQEAVy0M=";
          licenses = [ "COPYING" ];
        };
        icu = {
          url = "https://chromium.googlesource.com/chromium/deps/icu.git";
          rev = "d578f2e8b7bd5938e21cfb6bf15c079e0aa5b738";
          hash = "sha256-DsktUjKFFbeSqlFZTBqbZgvuJLOKvssCym2M3tcOJtw=";
          licenses = [ "LICENSE" ];
        };
        libjpeg-turbo = {
          url = "https://chromium.googlesource.com/chromium/deps/libjpeg_turbo.git";
          rev = "e14cbfaa85529d47f9f55b0f104a579c1061f9ad";
          hash = "sha256-Ig+tmprZDvlf/M72/DTar2pbxat9ZElgSqdXdoM0lPs=";
          licenses = [
            "LICENSE.md"
            "README.ijg"
          ];
        };
        libpng = {
          url = "https://skia.googlesource.com/third_party/libpng.git";
          rev = "d5515b5b8be3901aac04e5bd8bd5c89f287bcd33";
          hash = "sha256-N507gFyjXXsJ3dyQN0SlVy80WXlbNlZs/cnqLaCmFTk=";
          licenses = [ "LICENSE" ];
        };
        vulkanmemoryallocator = {
          url = "https://chromium.googlesource.com/external/github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator";
          rev = "eb744ea7a2b17040121b4bbb4d6f9e8a77e3cae7";
          hash = "sha256-LBZJcom7G7maF9wpUVeVEJQAJwGy6365INk3VD0/0PM=";
          licenses = [ "LICENSE.txt" ];
        };
        wuffs = {
          url = "https://skia.googlesource.com/external/github.com/google/wuffs-mirror-release-c.git";
          rev = "e3f919ccfe3ef542cfc983a82146070258fb57f8";
          hash = "sha256-373d2F/STcgCHEq+PO+SCHrKVOo6uO1rqqwRN5eeBCw=";
          licenses = [ "LICENSE" ];
        };
        zlib = {
          url = "https://chromium.googlesource.com/chromium/src/third_party/zlib";
          rev = "646b7f569718921d7d4b5b8e22572ff6c76f2596";
          hash = "sha256-jNj6SuTZ5/a7crtYhxW3Q/TlfRMNMfYIVxDlr7bYdzQ=";
          licenses = [ "LICENSE" ];
        };
      };
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
  lockedPackages = (lib.importTOML cargoLock).package;
  lockedForkCrates = lib.filter (package: lib.elem package.name bindings.crates) lockedPackages;
  lockedForkSources = lib.concatMapStringsSep " and " (
    package: "${package.name} from ${package.source or "a path"}"
  ) lockedForkCrates;
  forkPatches = lib.concatMapStringsSep "\n\n" (crate: ''
    [patch.crates-io.${crate}]
    git = "${bindings.url}"
    rev = "${bindings.rev}"'') bindings.crates;
  lockedBindingsVersions = map (package: package.version) (
    lib.filter (package: package.name == "skia-bindings") lockedForkCrates
  );
  dependencyTables =
    toml:
    let
      tables =
        attributes:
        map (kind: attributes.${kind} or { }) [
          "dependencies"
          "dev-dependencies"
          "build-dependencies"
        ];
    in
    tables toml ++ lib.concatMap tables (lib.attrValues (toml.target or { }));
  table = entry: if builtins.isString entry then { } else entry;
  skiaSafeEntries =
    root:
    let
      manifest = lib.importTOML (root + "/Cargo.toml");
      members = map (
        member:
        assert lib.assertMsg (!lib.hasInfix "*" member) ''
          The workspace member ${member} is a glob, so the skia-safe features of its crates cannot be read.
          List the members of the workspace by their paths.'';
        lib.importTOML (root + "/${member}/Cargo.toml")
      ) (manifest.workspace.members or [ ]);
      shared = table (manifest.workspace.dependencies.skia-safe or { });
      entries = lib.concatMap (
        toml:
        map (dependencies: table dependencies.skia-safe) (
          lib.filter (dependencies: dependencies ? skia-safe) (dependencyTables toml)
        )
      ) ([ manifest ] ++ members);
    in
    map (
      entry:
      if entry.workspace or false then
        {
          features = shared.features or [ ] ++ entry.features or [ ];
          default = shared.default-features or true;
        }
      else
        {
          features = entry.features or [ ];
          default = entry.default-features or true;
        }
    ) entries;
  entries = lib.concatMap skiaSafeEntries workspaces;
  enabledFeatures = lib.sort lib.lessThan (
    lib.unique (
      lib.concatMap (entry: entry.features) entries
      ++ lib.optional (lib.any (entry: entry.default) entries) "default"
    )
  );
  build =
    {
      name,
      platformGnArguments,
      externals,
      buildInputs ? [ ],
      environment ? { },
    }:
    assert lib.assertMsg (lockedBindingsVersions == [ bindings.version ]) ''
      ${toString cargoLock} locks skia-bindings ${toString lockedBindingsVersions},
      but nix/skia.nix builds Skia for skia-bindings ${bindings.version}.
      In nix/skia-bindings.nix, set the version, the commit and its hash to the locked release. In nix/skia.nix,
      set tag to package.metadata.skia of that release and the externals to the revisions in the DEPS file of the
      tag.'';
    assert lib.assertMsg
      (lib.all (package: (package.source or null) == bindings.cargoSource) lockedForkCrates)
      ''
        ${toString cargoLock} takes ${lockedForkSources},
        but the Skia builds need ${lib.concatStringsSep " and " bindings.crates} from ${bindings.cargoSource}.
        Add these lines to the Cargo.toml of the workspace root and update the lock file:

        ${forkPatches}'';
    assert lib.assertMsg (enabledFeatures == lib.attrNames features) ''
      The crates of ${toString workspaces} enable the skia-safe features ${toString enabledFeatures},
      but nix/skia.nix builds Skia for ${toString (lib.attrNames features)}.
      Enable exactly those features, or give the features of nix/skia.nix one entry for each enabled
      feature, with the gn arguments, libraries and defining targets that skia-bindings derives from it.'';
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
  hosts.x86_64-linux = "x64";
  system = pkgs.stdenv.hostPlatform.system;
in
{
  linux =
    assert lib.assertMsg (
      hosts ? ${system}
    ) "Skia is built for the Linux hosts ${toString (lib.attrNames hosts)}, not for ${system}.";
    build {
      name = "skia-${system}";
      externals = removeAttrs bundledExternals [ "freetype" ];
      platformGnArguments = {
        target_os = "linux";
        target_cpu = hosts.${system};
        skia_use_system_freetype2 = true;
        skia_system_freetype2_include_path = "${lib.getDev pkgs.freetype}/include/freetype2";
      };
      buildInputs = [
        pkgs.fontconfig
        pkgs.freetype
      ];
    };
  # Skia is built for the same API level as the app that links Skia. Both then import NDK symbols of one API level.
  android =
    { ndk, api }:
    assert lib.assertMsg (
      lib.isDerivation ndk || builtins.isPath ndk || (builtins.isString ndk && builtins.hasContext ndk)
    ) "The NDK of the AOSP builds of Skia must be in the Nix store, such as the NDK of androidenv.";
    let
      llvm = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
    in
    lib.mapAttrs (
      target: targetArguments:
      build {
        name = "skia-${target}";
        externals = bundledExternals;
        platformGnArguments = targetArguments.skia // {
          inherit ndk;
          ndk_api = api;
          # From API level 30 on, Skia would encode images through the NDK in place of the encoders bound by skia-safe.
          skia_use_ndk_images = false;
          skia_enable_fontmgr_android = true;
          skia_use_system_freetype2 = false;
        };
        environment = {
          ANDROID_NDK = toString ndk;
          CLANG_PATH = "${llvm}/bin/clang";
          LIBCLANG_PATH = "${llvm}/lib";
        };
      }
    ) (import ./android-targets.nix);
}
