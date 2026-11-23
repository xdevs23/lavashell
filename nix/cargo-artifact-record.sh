# SPDX-License-Identifier: GPL-3.0-only
recordCargoArtifacts() {
    local record="$1" cargoLogFd cargoLogPid
    shift
    exec {cargoLogFd}> >(tee -a "$record")
    cargoLogPid=$!
    "$@" >&"$cargoLogFd"
    exec {cargoLogFd}>&-
    wait "$cargoLogPid"
}
