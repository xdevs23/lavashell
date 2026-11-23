# SPDX-License-Identifier: GPL-3.0-only
cargoBuildType=packaged
cargoCheckType=packaged
NIX_BUILD_CORES=1
postBuildHooks=()
export NIX_BUILD_TOP="$PWD" out="$PWD/out" PATH="$PWD/bin:$PATH"
runHook() {
    case "$1" in
        preBuild) export PKG_CONFIG=retained-pkg-config ;;
        postBuild) for hook in "${postBuildHooks[@]}"; do "$hook"; done ;;
    esac
}
concatTo() {
    local -n destination="$1"
    shift
    local name words
    for name in "$@"; do
        local -n source="$name"
        read -ra words <<< "${source-}"
        destination+=("${words[@]}")
    done
}
prependToVar() {
    local -n variable="$1"
    shift
    variable="$* ${variable-}"
}
echoCmd() { printf '%s\n' "$*"; }
source build-hook.sh
source check-hook.sh
source install-hook.sh
for phase in $PHASES; do
    eval "${!phase}"
done
printf 'CHECK_PKG_CONFIG=%s\n' "${PKG_CONFIG-missing}"
printf 'CHECK_BINS=%s\n' "${bins-missing}"
cargoInstallHook
printf 'INSTALL_FINISHED\n'
