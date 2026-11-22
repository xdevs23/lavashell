# SPDX-License-Identifier: GPL-3.0-only
{
  pkgs,
  assets,
  version,
}:
let
  inherit (pkgs) lib;
  contract = import ./projection-contract.nix;
  fontConfig = pkgs.writeText "lavashell-fonts.conf" ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <include>${pkgs.fontconfig.out}/etc/fonts/conf.d</include>
      <dir>${pkgs.dejavu_fonts.minimal}/share/fonts/truetype</dir>
      <cachedir prefix="xdg">fontconfig</cachedir>
    </fontconfig>
  '';
  runtime = import ./runtime.nix { inherit pkgs; };
in
{
  inherit version;
  buildType = contract.profile;
  cargoBuildFlags = [ "--message-format=json-render-diagnostics" ];
  nativeBuildInputs = with pkgs; [
    pkg-config
    clang
    makeWrapper
  ];
  nativeCheckInputs = [ pkgs.dbus ];
  buildInputs = runtime.libraries;
  env =
    assets.environment
    // assets.testEnvironment
    // assets.waylandEnvironment
    // runtime.environment
    // {
      FONTCONFIG_FILE = "${fontConfig}";
    };
  preBuild =
    (import ./build-environment.nix {
      inherit pkgs;
      libraries = runtime.libraries;
    })
    + ''
      export NIX_CC=${pkgs.stdenv.cc}
      export CC=gcc CXX=g++
      export PATH="$NIX_CC/bin:$PATH"
    '';
  buildPhase = builtins.readFile ./cargo-build-record.sh;
  preCheck = ''
    export XDG_CACHE_HOME="$TMPDIR/lavashell-test-cache"
    mkdir -p "$XDG_CACHE_HOME"
  '';
  postInstall = ''
    if [ -d "$out/bin" ]; then
      for program in "$out/bin/"*; do
        wrapProgram "$program" \
          --set-default LAVASHELL_EXAMPLE_NODE_MODULES ${assets.nodeModules}/node_modules \
          --set-default FONTCONFIG_FILE ${fontConfig} \
          --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtime.libraries}
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
