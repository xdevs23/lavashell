# SPDX-License-Identifier: GPL-3.0-only
substitute() {
    sed "s|@$4@|$5|g" "$1" > "$2"
}
source "$CARGO_SETUP_HOOK"
cargoSetupPostUnpackHook
