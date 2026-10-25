# SPDX-License-Identifier: GPL-3.0-only
"""Tests that `process_scope` stops every process started in a scope, including a child that ignores SIGTERM or
outlives its parent. The packaged native launches rely on this cleanup. The tests need no network access."""

from pathlib import Path
import select
import signal
import subprocess
import sys
import unittest

import psutil

from process_scope import exit_event, first_exit, scoped_process


CHILD = """
import signal
import sys
if sys.argv[1] == 'ignore':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
print('ready', flush=True)
signal.pause()
"""

WRAPPER = f"""
import signal
import subprocess
import sys
child = subprocess.Popen([sys.executable, '-c', {CHILD!r}, sys.argv[1]],
                         stdout=subprocess.PIPE, text=True)
assert child.stdout.readline() == 'ready\\n'
print(child.pid, flush=True)
if sys.argv[2] != 'exit':
    signal.pause()
"""


class ProcessScopeTests(unittest.TestCase):
    def wrapper(self, *, ignore_term=False, exit_early=False):
        return scoped_process(
            [sys.executable, "-c", WRAPPER, "ignore" if ignore_term else "default",
             "exit" if exit_early else "stay"],
            stdout=subprocess.PIPE, text=True,
        )

    def assert_stopped(self, pid):
        try:
            status = psutil.Process(pid).status()
        except psutil.NoSuchProcess:
            return
        self.assertIn(status, {psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD},
                      f"fixture process {pid} survived scope cleanup")

    def test_scope_exit_stops_wrapper_and_long_lived_child(self):
        with self.wrapper() as wrapper:
            child = int(wrapper.stdout.readline())
        self.assertIsNotNone(wrapper.returncode)
        self.assert_stopped(child)

    def test_error_in_scope_stops_wrapper_and_long_lived_child(self):
        with self.assertRaises(RuntimeError), self.wrapper() as wrapper:
            child = int(wrapper.stdout.readline())
            raise RuntimeError("the scope body failed")
        self.assertIsNotNone(wrapper.returncode)
        self.assert_stopped(child)

    def test_exited_wrapper_does_not_leave_its_child_running(self):
        with self.wrapper(exit_early=True) as wrapper:
            child = int(wrapper.stdout.readline())
            self.assertEqual(wrapper.wait(), 0)
        self.assert_stopped(child)

    def test_child_ignoring_term_is_stopped(self):
        with self.wrapper(ignore_term=True) as wrapper:
            child = int(wrapper.stdout.readline())
        self.assert_stopped(child)

    def test_exit_event_is_readable_only_after_the_process_exited(self):
        with scoped_process(
            [sys.executable, "-c", "import sys; sys.stdin.read()"], stdin=subprocess.PIPE,
        ) as process, exit_event(process) as exited:
            self.assertEqual(select.select([exited], [], [], 0)[0], [])
            process.stdin.close()
            self.assertEqual(select.select([exited], [], [])[0], [exited])
            self.assertEqual(process.wait(), 0)

    def test_first_exit_returns_the_process_that_ended(self):
        with scoped_process(
            [sys.executable, "-c", "import sys; sys.stdin.read()"], stdin=subprocess.PIPE,
        ) as waiting, scoped_process([sys.executable, "-c", "pass"]) as finished:
            self.assertIs(first_exit(waiting, finished), finished)

    def test_harness_sigterm_unwinds_and_stops_separate_child_session(self):
        harness = f"""
import signal
import subprocess
import sys
from process_scope import scoped_process, termination_signals
with termination_signals():
    with scoped_process(
        [sys.executable, '-c', {WRAPPER!r}, 'ignore', 'stay'],
        stdout=subprocess.PIPE, text=True,
    ) as wrapper:
        child = wrapper.stdout.readline().strip()
        print(wrapper.pid, child, flush=True)
        signal.pause()
"""
        with scoped_process(
            [sys.executable, "-c", harness], cwd=Path(__file__).resolve().parent,
            stdout=subprocess.PIPE, text=True,
        ) as process:
            wrapper, child = map(int, process.stdout.readline().split())
            process.send_signal(signal.SIGTERM)
            self.assertEqual(process.wait(), 128 + signal.SIGTERM)
        self.assert_stopped(wrapper)
        self.assert_stopped(child)


if __name__ == "__main__":
    unittest.main()
