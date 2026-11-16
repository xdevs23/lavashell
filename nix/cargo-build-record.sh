# SPDX-License-Identifier: GPL-3.0-only
exec {cargoLogFd}> >(tee cargo-artifacts.jsonl)
cargoLogPid=$!
cargoBuildHook >&"$cargoLogFd"
exec {cargoLogFd}>&-
wait "$cargoLogPid"
unset cargoLogFd cargoLogPid
