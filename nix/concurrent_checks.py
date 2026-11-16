# SPDX-License-Identifier: GPL-3.0-only

import os
import resource
from collections.abc import Callable, Sequence
from concurrent.futures import FIRST_EXCEPTION, ThreadPoolExecutor, wait
from threading import Event

import psutil


def concurrent_count(count: int) -> int:
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024
    return max(1, min(count, os.process_cpu_count() or 1, psutil.virtual_memory().available // max(peak, 1)))


class Skipped(Exception):
    pass


def run_concurrently[T](checks: Sequence[Callable[[], T]]) -> list[T]:
    stopped = Event()

    def start(check: Callable[[], T]) -> T:
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
