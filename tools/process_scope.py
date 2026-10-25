# SPDX-License-Identifier: GPL-3.0-only
"""Run child processes in their own session and stop every process of that session on exit."""

from contextlib import ExitStack, contextmanager, suppress
import os
import select
import signal
import subprocess

import psutil


@contextmanager
def termination_signals():
    def terminate(number, _frame):
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        raise SystemExit(128 + number)

    previous = signal.signal(signal.SIGTERM, terminate)
    try:
        yield
    finally:
        signal.signal(signal.SIGTERM, previous)


def session_members(leader):
    """The live processes of the session that `leader` started, without the leader."""
    members = []
    for process in psutil.process_iter():
        with suppress(ProcessLookupError, psutil.Error):
            if (process.pid != leader.pid and os.getpgid(process.pid) == leader.pid
                    and process.status() not in {psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD}):
                members.append(process)
    return members


def kill_session(leader):
    """Kill every process of the session that `leader` started and return once all of them are gone."""
    while True:
        members = session_members(leader)
        with suppress(ProcessLookupError):
            os.killpg(leader.pid, signal.SIGKILL)
        if not members:
            break
        psutil.wait_procs(members)
    leader.wait()


@contextmanager
def scoped_process(command, **options):
    process = subprocess.Popen(command, start_new_session=True, **options)
    try:
        yield process
    finally:
        try:
            kill_session(process)
        finally:
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()


@contextmanager
def exit_event(process):
    """A file descriptor that becomes readable once the running `process` has exited."""
    descriptor = os.pidfd_open(process.pid)
    try:
        yield descriptor
    finally:
        os.close(descriptor)


def first_exit(*processes):
    """Block until one of the running `processes` has exited and return it."""
    with ExitStack() as scope:
        exited = {scope.enter_context(exit_event(process)): process for process in processes}
        readable, _, _ = select.select(list(exited), [], [])
        return exited[readable[0]]
