# SPDX-License-Identifier: GPL-3.0-only

from contextlib import AbstractContextManager
from pathlib import Path
import select
import signal
import subprocess
import sys
import unittest

import psutil

from process_scope import exit_event, first_exit, piped, scoped_process


TOOLS = Path(__file__).resolve().parent
WRAPPER = TOOLS / "process_wrapper.py"
HARNESS = TOOLS / "process_harness.py"
WAITS_FOR_INPUT = "import sys; sys.stdin.read()"


class ProcessScopeTests(unittest.TestCase):
    def wrapper(
        self, *, ignore_term: bool = False, exit_early: bool = False,
    ) -> AbstractContextManager[subprocess.Popen[str]]:
        flags = [*(["--ignore-term"] if ignore_term else []), *(["--exit"] if exit_early else [])]
        return scoped_process([sys.executable, str(WRAPPER), *flags], stdout=subprocess.PIPE, text=True)

    def assert_stopped(self, pid: int) -> None:
        try:
            status = psutil.Process(pid).status()
        except psutil.NoSuchProcess:
            return
        self.assertIn(status, {psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD},
                      f"fixture process {pid} survived scope cleanup")

    def test_scope_exit_stops_wrapper_and_long_lived_child(self) -> None:
        with self.wrapper() as wrapper:
            child = int(piped(wrapper.stdout).readline())
        self.assertIsNotNone(wrapper.returncode)
        self.assert_stopped(child)

    def test_error_in_scope_stops_wrapper_and_long_lived_child(self) -> None:
        with self.assertRaises(RuntimeError), self.wrapper() as wrapper:
            child = int(piped(wrapper.stdout).readline())
            raise RuntimeError("the scope body failed")
        self.assertIsNotNone(wrapper.returncode)
        self.assert_stopped(child)

    def test_exited_wrapper_does_not_leave_its_child_running(self) -> None:
        with self.wrapper(exit_early=True) as wrapper:
            child = int(piped(wrapper.stdout).readline())
            self.assertEqual(wrapper.wait(), 0)
        self.assert_stopped(child)

    def test_child_ignoring_term_is_stopped(self) -> None:
        with self.wrapper(ignore_term=True) as wrapper:
            child = int(piped(wrapper.stdout).readline())
        self.assert_stopped(child)

    def test_exit_event_is_readable_only_after_the_process_exited(self) -> None:
        with scoped_process(
            [sys.executable, "-c", WAITS_FOR_INPUT], stdin=subprocess.PIPE,
        ) as process, exit_event(process) as exited:
            self.assertEqual(select.select([exited], [], [], 0)[0], [])
            piped(process.stdin).close()
            self.assertEqual(select.select([exited], [], [])[0], [exited])
            self.assertEqual(process.wait(), 0)

    def test_first_exit_returns_the_process_that_ended(self) -> None:
        with scoped_process(
            [sys.executable, "-c", WAITS_FOR_INPUT], stdin=subprocess.PIPE,
        ) as waiting, scoped_process([sys.executable, "-c", "pass"]) as finished:
            self.assertIs(first_exit(waiting, finished), finished)

    def test_harness_sigterm_unwinds_and_stops_separate_child_session(self) -> None:
        with scoped_process(
            [sys.executable, str(HARNESS)], stdout=subprocess.PIPE, text=True,
        ) as process:
            wrapper, child = map(int, piped(process.stdout).readline().split())
            process.send_signal(signal.SIGTERM)
            self.assertEqual(process.wait(), 128 + signal.SIGTERM)
        self.assert_stopped(wrapper)
        self.assert_stopped(child)


if __name__ == "__main__":
    unittest.main()
