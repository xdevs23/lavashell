# SPDX-License-Identifier: GPL-3.0-only
# Runs cargoBuildHook in the shell of the build phase and copies its output into cargo-artifacts.jsonl through tee.
# A pipe would run the hook in a subshell and lose the variables that the hook sets and exports. Only tee runs as
# a child process.
exec {cargoLogFd}> >(tee cargo-artifacts.jsonl)
cargoLogPid=$!
cargoBuildHook >&"$cargoLogFd"
exec {cargoLogFd}>&-
wait "$cargoLogPid"
unset cargoLogFd cargoLogPid
