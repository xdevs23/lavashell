# SPDX-License-Identifier: GPL-3.0-only
recordCargoArtifacts() {
    local record="$1" cargoOutputFd cargoOutputPid
    shift
    exec {cargoOutputFd}> >(jq --null-input --raw-input --raw-output --unbuffered '
        def cargoMessage: (fromjson? | objects) // null;
        def writeToBuildLog: "\(.)\n" | stderr | empty;
        (label $buildFinished
            | inputs
            | cargoMessage as $message
            | if $message == null then writeToBuildLog
              else ., if $message.reason == "build-finished" then break $buildFinished else empty end
              end),
        (inputs | writeToBuildLog)
    ' >>"$record")
    cargoOutputPid=$!
    "$@" >&"$cargoOutputFd"
    exec {cargoOutputFd}>&-
    wait "$cargoOutputPid"
}
