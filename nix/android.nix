# SPDX-License-Identifier: GPL-3.0-only
{
  toolchain,
  skia,
  root ? ../.,
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
    emulatorApi
    nativeApi
    pageSize
    pageSizeKiB
    buildToolsVersion
    emulatorSdk
    ;
  inherit (pkgs) lib;
  target = "aarch64-linux-android";
  emulatorTarget = "x86_64-linux-android";
  targets = import ./android-targets.nix;
  assets = import ./assets.nix { inherit pkgs; };
  sources = import ./source.nix { inherit pkgs root; };
  notices = import ./notices.nix { inherit pkgs sources root; };
  targetEnvironment =
    target:
    let
      variable = lib.toUpper (lib.replaceStrings [ "-" ] [ "_" ] target);
      suffix = lib.replaceStrings [ "-" ] [ "_" ] target;
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
    pkgs.python3
    pkgs.zip
    pkgs.jdk17
    pkgs.kotlin
  ];
  environment = targetEnvironment target // toolchainEnvironment // assets.environment;
  buildTools = "${sdkRoot}/build-tools/${buildToolsVersion}";
  androidJar = "${sdkRoot}/platforms/android-${platform}/android.jar";
  abi = targets.${emulatorTarget}.abi;
  architecture = builtins.head (lib.splitString "-" emulatorTarget);
  kotlinStdlib = "${pkgs.kotlin}/lib/kotlin-stdlib.jar";
  # The JVM part of the Kotlin standard library contains third-party code under the four licences after NOTICE.txt.
  kotlinStdlibLicenses = map (name: "${pkgs.kotlin}/license/${name}") [
    "LICENSE.txt"
    "NOTICE.txt"
    "third_party/boost_LICENSE.txt"
    "third_party/guava_license.txt"
    "third_party/gwt_license.txt"
    "third_party/threetenbp_license.txt"
  ];
  # The round trip builds in the projected workspace of the native examples, because that workspace contains
  # lavashell-android and its lock file resolves the development dependencies of the round trips.
  roundTripWorkspace = root + "/examples/support";
  roundTrip = rustPlatform.buildRustPackage {
    pname = "lavashell-surface-stream-round-trip";
    version = "0.1.0";
    src = sources.sourceFor roundTripWorkspace;
    cargoDeps = sources.vendor (sources.project roundTripWorkspace).lockFile;
    nativeBuildInputs = packageTools;
    buildInputs = [ skia.${emulatorTarget} ];
    env = targetEnvironment emulatorTarget // toolchainEnvironment;
    doCheck = false;
    buildPhase = ''
      runHook preBuild
      cargo build --frozen --target ${emulatorTarget} -p lavashell-android \
        --example surface-stream-round-trip
      mkdir -p android-classes android-dex
      kotlinc -Werror -no-jdk -no-reflect -classpath ${androidJar} \
        -d android-classes crates/lavashell-android/emulator/*.kt
      ${buildTools}/d8 --lib ${androidJar} \
        --min-api ${toString nativeApi} --output android-dex \
        android-classes/org/lavashell/surfacestream/*.class ${kotlinStdlib}
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/share/android" apk/lib/${abi}
      cp target/${emulatorTarget}/debug/examples/libsurface_stream_round_trip.so apk/lib/${abi}/
      ${buildTools}/aapt2 link -I ${androidJar} \
        --manifest crates/lavashell-android/emulator/AndroidManifest.xml \
        --min-sdk-version ${toString nativeApi} --target-sdk-version ${targetSdk} -o apk/unaligned.apk
      cp android-dex/classes.dex apk/
      (cd apk && zip -q -0 unaligned.apk classes.dex lib/${abi}/libsurface_stream_round_trip.so)
      apk="$out/share/android/surface-stream-round-trip-unsigned.apk"
      ${buildTools}/zipalign -P ${toString pageSizeKiB} -f 4 apk/unaligned.apk "$apk"
      ${buildTools}/zipalign -c -P ${toString pageSizeKiB} 4 "$apk"
      report="$out/share/android/library-check.json"
      if ! python3 ${
        root + "/tools/check-android-library.py"
      } apk/lib/${abi}/libsurface_stream_round_trip.so \
        --llvm ${llvmRoot}/bin --system-libraries ${llvmRoot}/sysroot/usr/lib/${emulatorTarget} \
        --arch ${architecture} --api-level ${toString nativeApi} \
        --page-size ${toString pageSize} > "$report"; then
        cat "$report" >&2
        exit 1
      fi
      install -Dm444 COPYING "$out/share/licenses/lavashell/COPYING"
      install -Dm444 ${
        notices {
          directory = roundTripWorkspace;
          member = root + "/crates/lavashell-android";
          target = emulatorTarget;
        }
      } "$out/share/licenses/lavashell/rust-crates.txt"
      ${skia.${emulatorTarget}.installLicenses ''"$out/share/licenses/skia"''}
      install -Dm444 -t "$out/share/licenses/kotlin-stdlib" ${lib.escapeShellArgs kotlinStdlibLicenses}
      runHook postInstall
    '';
    dontFixup = true;
    meta.license = lib.licenses.gpl3Only;
  };
  emulatorTools = lib.fileset.toSource {
    root = root + "/tools";
    fileset = lib.fileset.unions (
      map (name: root + "/tools/${name}") [
        "android-apk.py"
        "android-surface-stream-test.py"
        "process_scope.py"
      ]
    );
  };
  roundTripRunner = pkgs.writeShellApplication {
    name = "surface-stream-round-trip";
    runtimeInputs = [
      (pkgs.python3.withPackages (python: [
        python.cryptography
        python.psutil
      ]))
      pkgs.jdk17
    ];
    text = ''
      exec python3 ${emulatorTools}/android-surface-stream-test.py \
        --apk ${roundTrip}/share/android/surface-stream-round-trip-unsigned.apk \
        --signer ${emulatorTools}/android-apk.py \
        --sdk ${lib.escapeShellArg sdkRoot} \
        --build-tools ${lib.escapeShellArg buildToolsVersion} \
        --emulator-sdk ${emulatorSdk}/libexec/android-sdk \
        --api ${emulatorApi} --page-size-kib ${toString pageSizeKiB} "$@"
    '';
  };
in
{
  packages.surface-stream-round-trip = roundTrip;
  apps.surface-stream-round-trip = {
    type = "app";
    program = lib.getExe roundTripRunner;
    meta.description = "Run the round trips of frames through streams in a headless AOSP emulator";
    meta.lavashellHost = "android";
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
      shellHook = skia.${target}.shellHook;
    }
    // lib.mapAttrs' (
      buildTarget: build:
      let
        variable = lib.toUpper (lib.replaceStrings [ "-" ] [ "_" ] buildTarget);
      in
      lib.nameValuePair "LAVASHELL_SKIA_ACTIVATE_${variable}" build.activate
    ) skia
  );
}
