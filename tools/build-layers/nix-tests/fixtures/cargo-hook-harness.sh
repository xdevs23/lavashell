# SPDX-License-Identifier: GPL-3.0-only
cargoBuildType=packaged
NIX_BUILD_CORES=1
cargoBuildFlags=(--message-format=json-render-diagnostics)
postBuildHooks=()
export NIX_BUILD_TOP="$PWD" out="$PWD/out"
runHook() {
    case "$1" in
        preBuild) export PKG_CONFIG=retained-pkg-config ;;
        postBuild) for hook in "${postBuildHooks[@]}"; do "$hook"; done ;;
    esac
}
concatTo() {
    local -n destination="$1" source="$2"
    destination+=("${source[@]}")
}
echoCmd() { printf '%s\n' "$*"; }
cargo() {
    if [ "$CARGO_FIXTURE_STATUS" != 0 ]; then
        printf '{"reason":"build-finished","success":false}\n'
        return "$CARGO_FIXTURE_STATUS"
    fi
    mkdir -p target/x86_64-unknown-linux-gnu/packaged
    printf '#!/bin/sh\nexit 0\n' > target/x86_64-unknown-linux-gnu/packaged/fixture
    chmod +x target/x86_64-unknown-linux-gnu/packaged/fixture
    printf '{"reason":"build-finished","success":true}\n'
}
source build-hook.sh
source install-hook.sh
eval "$buildPhase"
printf 'CHECK_PKG_CONFIG=%s\n' "${PKG_CONFIG-missing}"
printf 'CHECK_BINS=%s\n' "${bins-missing}"
cargoInstallHook
printf 'INSTALL_FINISHED\n'
