# SPDX-License-Identifier: GPL-3.0-only
"""Fuzz the targets of the protocol crate until the run is stopped.

The fuzzers run at idle priority, one on each CPU that the run may use and at least one for each
target, so the run uses only the time that no other process uses. The fuzzers of a target share
the corpus in `<corpus>/<target>`, which persists between runs. A fuzzer that exits stops the run,
and a crash leaves the input that caused it in `<corpus>/artifacts/<target>`.

    nix run .#protocol-fuzz -- <corpus> [target ...]
"""

import argparse
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from contextlib import suppress
import os
from pathlib import Path
import signal
import subprocess
import sys
import tomllib

FUZZ = Path(__file__).resolve().parent
BUILD = ("--sanitizer", "none", "--debug-assertions")
# Inputs of every length from the start, corpus inputs that are never shortened, and coverage as
# the set of edges hit, so that the corpus grows only with new edges. Neither the time nor the
# memory of an input is limited.
FLAGS = ("-len_control=0", "-reduce_inputs=0", "-use_counters=0", "-timeout=0", "-rss_limit_mb=0")


class FuzzerFailed(Exception):
    def __init__(self, target: str, code: int, crashes: list[Path]):
        found = f", crash inputs: {', '.join(map(str, crashes))}" if crashes else ", no crash input"
        super().__init__(f"the fuzzer of {target} exited with {code}{found}")


def targets() -> list[str]:
    with open(FUZZ / "Cargo.toml", "rb") as manifest:
        return [binary["name"] for binary in tomllib.load(manifest)["bin"]]


def assignments(names: list[str], cpus: int) -> list[str]:
    """The target of each fuzzer: one fuzzer for each CPU, at least one for each target, and the
    targets in turn."""
    return [names[index % len(names)] for index in range(max(cpus, len(names)))]


def largest_input(available: int, fuzzers: int) -> int:
    """The longest input, in bytes, for a share of the available memory: libFuzzer fills a buffer
    of that length for each crossover, which leaves the other half of the share for the corpus and
    the target."""
    return max(available // fuzzers // 2, 1)


def stop(processes: list[subprocess.Popen]) -> None:
    """Stops the session of each fuzzer that still runs, which contains the fuzzer that
    `cargo fuzz` started."""
    for process in processes:
        if process.returncode is None:
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
    for process in processes:
        process.wait()


def command(corpus: Path, name: str, max_len: int) -> list[str]:
    return [
        "cargo", "fuzz", "run", *BUILD, name, str(corpus / name), "--", *FLAGS,
        f"-max_len={max_len}", f"-artifact_prefix={corpus / 'artifacts' / name}/",
    ]


def fuzz(corpus: Path, names: list[str]) -> None:
    for name in names:
        subprocess.run(["cargo", "fuzz", "build", *BUILD, name], cwd=FUZZ, check=True)
        (corpus / name).mkdir(parents=True, exist_ok=True)
        (corpus / "artifacts" / name).mkdir(parents=True, exist_ok=True)
    known = {path for name in names for path in (corpus / "artifacts" / name).iterdir()}
    fuzzers = assignments(names, len(os.sched_getaffinity(0)))
    available = os.sysconf("SC_AVPHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    max_len = largest_input(available, len(fuzzers))
    commands = {name: command(corpus, name, max_len) for name in names}
    processes: list[subprocess.Popen] = []
    exits = {}
    with ThreadPoolExecutor(len(fuzzers)) as pool:
        try:
            for name in fuzzers:
                processes.append(subprocess.Popen(commands[name], cwd=FUZZ, start_new_session=True))
                exits[pool.submit(processes[-1].wait)] = name
            first = next(iter(wait(exits, return_when=FIRST_COMPLETED).done))
        finally:
            stop(processes)
    name = exits[first]
    crashes = sorted(set((corpus / "artifacts" / name).iterdir()) - known)
    raise FuzzerFailed(name, first.result(), crashes)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "corpus", type=Path, help="the directory of the corpora, outside the source tree"
    )
    parser.add_argument(
        "targets", nargs="*", help="the targets to fuzz, every target when none is named"
    )
    arguments = parser.parse_args()
    signal.signal(signal.SIGTERM, lambda number, _frame: sys.exit(f"stopped by signal {number}"))
    os.sched_setscheduler(0, os.SCHED_IDLE, os.sched_param(0))
    fuzz(arguments.corpus.resolve(), arguments.targets or targets())


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit("stopped")
    except (FuzzerFailed, subprocess.CalledProcessError) as failure:
        sys.exit(str(failure))
