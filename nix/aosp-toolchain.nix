# SPDX-License-Identifier: GPL-3.0-only
{
  nixpkgs,
  rust-overlay,
  system,
  root,
}:
let
  pkgs = import nixpkgs {
    inherit system;
    overlays = [ rust-overlay.overlays.default ];
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
  rust = pkgs.rust-bin.stable."1.99.0".default.override {
    targets = builtins.attrNames (import ./aosp-targets.nix);
  };
  sdk =
    (pkgs.androidenv.composeAndroidPackages {
      platformVersions = [ platform ];
      buildToolsVersions = [ buildToolsVersion ];
      includeNDK = true;
      ndkVersions = [ ndkVersion ];
      includeEmulator = false;
    }).androidsdk;
  sdkRoot = "${sdk}/libexec/android-sdk";
  ndkRoot = "${sdkRoot}/ndk/${ndkVersion}";
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
    sdk
    sdkRoot
    ndkRoot
    emulatorSdk
    ;
  llvmRoot = "${ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64";
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
}
