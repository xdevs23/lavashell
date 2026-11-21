# SPDX-License-Identifier: GPL-3.0-only
{
  toolchain,
  skia,
  root,
  buildLayers,
  aospRoundTrip,
}:
let
  inherit (toolchain)
    pkgs
    rust
    rustPlatform
    sdk
    sdkRoot
    ndkRoot
    llvmRoot
    platform
    targetSdk
    nativeApi
    pageSize
    pageSizeKiB
    zipAlignment
    buildToolsVersion
    emulatorSdk
    ;
  inherit (pkgs) lib;
  types = import ./types.nix;
  targets = import ./aosp-targets.nix;
  targetWithRole =
    role:
    let
      matching = lib.attrNames (lib.filterAttrs (_: candidate: candidate.role == role) targets);
    in
    assert lib.assertMsg (
      builtins.length matching == 1
    ) "nix/aosp-targets.nix must have exactly one target with the role ${role}.";
    builtins.head matching;
  deviceTarget = targetWithRole types.targetRoles.device;
  emulatorTarget = targetWithRole types.targetRoles.emulator;
  underscored = lib.replaceStrings [ "-" ] [ "_" ];
  assets = import ./assets.nix { inherit pkgs; };
  sources = import ./source.nix {
    inherit pkgs root;
    projections = import ./checkout-projections.nix root;
    commits = import ./vendored-commits.nix;
  };
  notices = import ./notices.nix {
    inherit
      pkgs
      rustPlatform
      sources
      root
      ;
  };
  targetEnvironment =
    target:
    let
      suffix = underscored target;
      variable = lib.toUpper suffix;
      clang = "${llvmRoot}/bin/${target}${toString nativeApi}-clang";
    in
    {
      "CARGO_TARGET_${variable}_LINKER" = clang;
      "CC_${suffix}" = clang;
      "CXX_${suffix}" = "${clang}++";
      "AR_${suffix}" = "${llvmRoot}/bin/llvm-ar";
      "CARGO_TARGET_${variable}_RUSTFLAGS" = lib.concatStringsSep " " [
        "-C link-arg=-Wl,-z,max-page-size=${toString pageSize}"
        "-C link-arg=-Wl,-z,common-page-size=${toString pageSize}"
      ];
    };
  toolchainEnvironment = {
    ANDROID_HOME = sdkRoot;
    ANDROID_NDK_HOME = ndkRoot;
  };
  packageTools = [
    pkgs.clang
    pkgs.pkg-config
    pkgs.zip
    pkgs.jdk17
    pkgs.kotlin
  ];
  environment = targetEnvironment deviceTarget // toolchainEnvironment // assets.environment;
  buildTools = "${sdkRoot}/build-tools/${buildToolsVersion}";
  aospJar = "${sdkRoot}/platforms/android-${platform}/android.jar";
  abi = targets.${emulatorTarget}.abi;
  inherit (targets.${emulatorTarget}) architecture;
  kotlinStdlib = "${pkgs.kotlin}/lib/kotlin-stdlib.jar";
  # https://github.com/JetBrains/kotlin/tree/master/license
  kotlinStdlibLicenses = map (name: "${pkgs.kotlin}/license/${name}") [
    "LICENSE.txt"
    "NOTICE.txt"
    "third_party/boost_LICENSE.txt"
    "third_party/guava_license.txt"
    "third_party/gwt_license.txt"
    "third_party/threetenbp_license.txt"
  ];
  roundTripWorkspace = sources.nativeDirectory;
  instrumentation = {
    package = "org.lavashell.surfacestream";
    runner = "StreamRoundTripRunner";
  };
  manifest = pkgs.writeText "manifest.xml" ''
    <?xml version="1.0" encoding="utf-8"?>
    <manifest xmlns:android="http://schemas.android.com/apk/res/android"
        package="${instrumentation.package}" android:versionCode="1" android:versionName="0.1.0">
        <uses-feature android:name="android.hardware.vulkan.version" android:version="4198400" android:required="true" />
        <application android:label="Surface stream round trips" android:extractNativeLibs="false"
            android:allowBackup="false">
            <activity android:name=".StreamRoundTripActivity" android:exported="false"
                android:configChanges=
                    "orientation|keyboardHidden|screenSize|screenLayout|smallestScreenSize|density|uiMode" />
        </application>
        <instrumentation android:name=".${instrumentation.runner}" android:targetPackage="${instrumentation.package}" />
    </manifest>
  '';
  crossBuild =
    {
      pname,
      target,
      buildPhase,
      installPhase,
    }:
    rustPlatform.buildRustPackage {
      inherit pname buildPhase installPhase;
      inherit (sources) version;
      src = sources.sourceFor roundTripWorkspace;
      cargoDeps = sources.vendor (sources.project roundTripWorkspace).lockFile;
      nativeBuildInputs = packageTools;
      buildInputs = [ skia.${target} ];
      env = targetEnvironment target // toolchainEnvironment;
      doCheck = false;
      dontFixup = true;
      meta.license = lib.licenses.gpl3Only;
    };
  deviceLibrary = crossBuild {
    pname = "lavashell-aosp-library";
    target = deviceTarget;
    buildPhase = ''
      runHook preBuild
      cargo build --frozen --target ${deviceTarget} -p lavashell-aosp
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm444 -t "$out/lib" target/${deviceTarget}/debug/liblavashell_aosp.rlib
      runHook postInstall
    '';
  };
  roundTrip = crossBuild {
    pname = "lavashell-surface-stream-round-trip";
    target = emulatorTarget;
    buildPhase = ''
      runHook preBuild
      cargo build --frozen --target ${emulatorTarget} -p lavashell-aosp \
        --example surface-stream-round-trip
      mkdir -p aosp-classes aosp-dex
      kotlinc -Werror -no-jdk -no-reflect -classpath ${aospJar} \
        -d aosp-classes crates/lavashell-aosp/emulator/*.kt
      ${buildTools}/d8 --lib ${aospJar} \
        --min-api ${toString nativeApi} --output aosp-dex \
        aosp-classes/${lib.replaceStrings [ "." ] [ "/" ] instrumentation.package}/*.class ${kotlinStdlib}
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/share/aosp" apk/lib/${abi}
      cp target/${emulatorTarget}/debug/examples/libsurface_stream_round_trip.so apk/lib/${abi}/
      ${buildTools}/aapt2 link -I ${aospJar} \
        --manifest ${manifest} \
        --min-sdk-version ${toString nativeApi} --target-sdk-version ${targetSdk} -o apk/unaligned.apk
      cp aosp-dex/classes.dex apk/
      (cd apk && zip -q -0 unaligned.apk classes.dex lib/${abi}/libsurface_stream_round_trip.so)
      apk="$out/share/aosp/surface-stream-round-trip-unsigned.apk"
      ${buildTools}/zipalign -P ${toString pageSizeKiB} -f ${toString zipAlignment} apk/unaligned.apk "$apk"
      ${buildTools}/zipalign -c -P ${toString pageSizeKiB} ${toString zipAlignment} "$apk"
      report="$out/share/aosp/library-check.json"
      if ! ${lib.getExe' buildLayers "lavashell-aosp-library-check"} apk/lib/${abi}/libsurface_stream_round_trip.so \
        --system-libraries ${llvmRoot}/sysroot/usr/lib/${emulatorTarget} \
        --arch ${architecture} --api-level ${toString nativeApi} \
        --page-size ${toString pageSize} > "$report"; then
        cat "$report" >&2
        exit 1
      fi
      install -Dm444 COPYING "$out/share/licenses/lavashell/COPYING"
      install -Dm444 ${
        notices {
          directory = roundTripWorkspace;
          member = root + "/crates/lavashell-aosp";
          target = emulatorTarget;
        }
      } "$out/share/licenses/lavashell/rust-crates.txt"
      ${skia.${emulatorTarget}.installLicenses ''"$out/share/licenses/skia"''}
      install -Dm444 -t "$out/share/licenses/kotlin-stdlib" ${lib.escapeShellArgs kotlinStdlibLicenses}
      runHook postInstall
    '';
  };
  roundTripRunner = pkgs.writeShellApplication {
    name = "surface-stream-round-trip";
    runtimeInputs = [ pkgs.jdk17 ];
    text = ''
      exec ${lib.getExe aospRoundTrip} \
        --apk ${roundTrip}/share/aosp/surface-stream-round-trip-unsigned.apk \
        --sdk ${lib.escapeShellArg sdkRoot} \
        --build-tools ${lib.escapeShellArg buildToolsVersion} \
        --emulator-sdk ${emulatorSdk}/libexec/android-sdk \
        --abi ${abi} --page-size ${toString pageSize} \
        --alignment ${toString zipAlignment} \
        --runner ${instrumentation.package}/.${instrumentation.runner} "$@"
    '';
  };
in
{
  packages = {
    aosp-library = deviceLibrary;
    surface-stream-round-trip = roundTrip;
  };
  apps.surface-stream-round-trip = {
    type = "app";
    program = lib.getExe roundTripRunner;
    meta.description = "Run the round trips of frames through streams in a headless AOSP emulator";
    meta.lavashellHost = types.hosts.aosp;
  };
  shell = pkgs.mkShell (
    environment
    // {
      packages = [
        rust
        sdk
        pkgs.nixfmt
      ]
      ++ packageTools;
      shellHook = skia.${deviceTarget}.shellHook;
    }
    // lib.mapAttrs' (
      buildTarget: build:
      let
        variable = lib.toUpper (underscored buildTarget);
      in
      lib.nameValuePair "LAVASHELL_SKIA_ACTIVATE_${variable}" build.activate
    ) skia
  );
}
