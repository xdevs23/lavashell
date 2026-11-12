# SPDX-License-Identifier: GPL-3.0-only

import os
import resource
from concurrent.futures import FIRST_EXCEPTION, ThreadPoolExecutor, wait
from pathlib import Path
from threading import Event


def available_memory():
    """The memory that new processes can use without swapping, page cache included, as the kernel estimates it."""
    fields = dict(line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines())
    kibibytes, _unit_always_kb = fields["MemAvailable"].split()
    return int(kibibytes) * 1024


def concurrent_count(count):
    """One of `count` checks per processor, as many as the available memory holds at the peak resident size of the
    largest child process that has finished so far."""
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024
    return max(1, min(count, os.process_cpu_count() or 1, available_memory() // max(peak, 1)))


class Skipped(Exception):
    """A check that did not start, because another check had failed or the caller was interrupted."""


def run_concurrently(checks):
    """Runs every check and returns their results in order. Once a check fails or the caller is interrupted, the checks
    that have not started are skipped, and the failure of the first failed check in order is raised."""
    stopped = Event()

    def start(check):
        if stopped.is_set():
            raise Skipped
        try:
            return check()
        except BaseException:
            stopped.set()
            raise

    with ThreadPoolExecutor(max_workers=concurrent_count(len(checks))) as pool:
        try:
            running = [pool.submit(start, check) for check in checks]
            wait(running, return_when=FIRST_EXCEPTION)
        finally:
            stopped.set()
    errors = (check.exception() for check in running)
    if failure := next((error for error in errors if error and not isinstance(error, Skipped)), None):
        raise failure
    return [check.result() for check in running]
