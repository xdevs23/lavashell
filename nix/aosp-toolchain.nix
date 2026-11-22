# SPDX-License-Identifier: GPL-3.0-only
{
  nixpkgs,
  rustToolchain,
  system,
  root,
}:
let
  pkgs = import nixpkgs {
    inherit system;
    config = {
      android_sdk.accept_license = true;
      allowUnfree = true;
    };
  };
  inherit (pkgs) lib;
  apiLevelFeatures = lib.filter (lib.hasPrefix "api-level-") (
    (builtins.fromTOML (builtins.readFile (root + "/Cargo.toml"))).workspace.dependencies.ndk.features
  );
  nativeApi =
    assert lib.assertMsg (
      builtins.length apiLevelFeatures == 1
    ) "The ndk dependency of the workspace needs exactly one api-level feature.";
    lib.toInt (lib.removePrefix "api-level-" (builtins.head apiLevelFeatures));
  pageSize = 16384;
  pageSizeKiB = pageSize / 1024;
  zipAlignment = 4;
  platform = builtins.head (pkgs.androidenv.composeAndroidPackages { }).platformVersions;
  targetSdk = lib.versions.major platform;
  imageApis = lib.concatLists (
    lib.mapAttrsToList (
      key: types:
      let
        api = builtins.match "v([0-9]+)" key;
      in
      lib.optional (api != null && types.default.x86_64 or null != null) (lib.toInt (builtins.head api))
    ) (pkgs.androidenv.composeAndroidPackages { }).all.system-images
  );
  emulatorApi = toString (lib.last (lib.sort lib.lessThan imageApis));
  buildToolsVersion = "36.0.0";
  ndkVersion = "28.2.13676358";
  inherit (rustToolchain { targets = builtins.attrNames (import ./aosp-targets.nix); })
    rust
    rustPlatform
    ;
  sdk =
    (pkgs.androidenv.composeAndroidPackages {
      platformVersions = [ platform ];
      buildToolsVersions = [ buildToolsVersion ];
      includeNDK = true;
      ndkVersions = [ ndkVersion ];
      includeEmulator = false;
    }).androidsdk;
  sdkPath = "/libexec/android-sdk";
  ndkPath = "${sdkPath}/ndk/${ndkVersion}";
  sdkRoot = "${sdk}${sdkPath}";
  ndkRoot = "${sdk}${ndkPath}";
  emulatorSdk =
    (pkgs.androidenv.composeAndroidPackages {
      platformVersions = [ emulatorApi ];
      includeEmulator = true;
      includeSystemImages = true;
      systemImageTypes = [ "default" ];
      abiVersions = [ "x86_64" ];
    }).androidsdk;
in
{
  inherit
    pkgs
    platform
    targetSdk
    nativeApi
    pageSize
    pageSizeKiB
    zipAlignment
    buildToolsVersion
    rust
    rustPlatform
    sdk
    sdkRoot
    ndkPath
    ndkRoot
    emulatorSdk
    ;
  llvmRoot = "${ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64";
}
