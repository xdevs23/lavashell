# SPDX-License-Identifier: GPL-3.0-only

import os
import resource
from concurrent.futures import ThreadPoolExecutor


def concurrent_count(count):
    """One of `count` checks per processor, as many as the free memory holds at the peak resident size of the
    largest child process that has finished so far."""
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024
    free = os.sysconf("SC_AVPHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    return max(1, min(count, os.process_cpu_count() or 1, free // max(peak, 1)))


def run_concurrently(checks):
    """Runs every check, returns their results in order, and raises the failure of the first check that failed."""
    with ThreadPoolExecutor(max_workers=concurrent_count(len(checks))) as pool:
        return [running.result() for running in [pool.submit(check) for check in checks]]
