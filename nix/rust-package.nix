# SPDX-License-Identifier: GPL-3.0-only
{ pkgs }:
let
  inherit (pkgs) lib;
  assets = import ./assets.nix { inherit pkgs; };
  privateBus = pkgs.writeShellScript "lavashell-private-test-dbus" ''
    if [ "''${1-}" = --session ]; then
      shift
    fi
    exec ${pkgs.dbus}/bin/dbus-daemon \
      --config-file=${pkgs.dbus}/share/dbus-1/session.conf "$@"
  '';
  fontConfig = pkgs.writeText "lavashell-fonts.conf" ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <include>${pkgs.fontconfig.out}/etc/fonts/conf.d</include>
      <dir>${pkgs.dejavu_fonts.minimal}/share/fonts/truetype</dir>
      <cachedir prefix="xdg">fontconfig</cachedir>
    </fontconfig>
  '';
  runtime = with pkgs; [
    fontconfig
    freetype
    wayland
    libxkbcommon
    vulkan-loader
  ];
in
{
  version = "0.1.0";
  buildType = "packaged";
  cargoBuildFlags = [ "--message-format=json-render-diagnostics" ];
  nativeBuildInputs = with pkgs; [
    pkg-config
    clang
    makeWrapper
  ];
  nativeCheckInputs = [ pkgs.dbus ];
  buildInputs = runtime;
  env =
    assets.environment
    // assets.waylandEnvironment
    // {
      LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
      FONTCONFIG_FILE = "${fontConfig}";
    };
  preBuild =
    (import ./build-environment.nix {
      inherit pkgs;
      libraries = runtime;
    })
    + ''
      export NIX_CC=${pkgs.stdenv.cc}
      export CC=gcc CXX=g++
      export PATH="$NIX_CC/bin:$PATH"
    '';
  buildPhase = builtins.readFile ./cargo-build-record.sh;
  preCheck = ''
    export DBUS_DAEMON="${privateBus}"
    export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.${pkgs.stdenv.hostPlatform.parsed.cpu.name}.json"
    export LAVASHELL_TEST_FONT="${assets.testEnvironment.LAVASHELL_TEST_FONT}"
    export XDG_CACHE_HOME="$TMPDIR/lavashell-test-cache"
    mkdir -p "$XDG_CACHE_HOME"
  '';
  postInstall = ''
    if [ -d "$out/bin" ]; then
      for program in "$out/bin/"*; do
        wrapProgram "$program" \
          --set-default LAVASHELL_EXAMPLE_NODE_MODULES ${assets.nodeModules}/node_modules \
          --set-default FONTCONFIG_FILE ${fontConfig} \
          --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtime}
      done
    fi
    install -Dm444 COPYING "$out/share/licenses/lavashell/COPYING"
    install -Dm444 cargo-artifacts.jsonl "$out/share/lavashell/cargo-artifacts.jsonl"
  '';
  meta = {
    description = "Lavashell native UI examples";
    license = lib.licenses.gpl3Only;
    platforms = [ "x86_64-linux" ];
  };
}
