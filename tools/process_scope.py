# SPDX-License-Identifier: GPL-3.0-only

from collections.abc import Iterator
from contextlib import ExitStack, contextmanager, suppress
import os
import select
import signal
import subprocess
from types import FrameType
from typing import IO, Any, NoReturn

import psutil

from next_steps import NextStep, ToolError


class CommandFailed(ToolError):
    def __init__(self, command: list[str], exit_code: int, output: str = "") -> None:
        super().__init__(f"{command} exited {exit_code}: {output}", NextStep.READ_RUN_LOGS)


class PipeNotRequested(Exception):
    def __init__(self) -> None:
        super().__init__("the process was started without a pipe for this stream")


def piped[Content: (str, bytes)](stream: IO[Content] | None) -> IO[Content]:
    if stream is None:
        raise PipeNotRequested()
    return stream


@contextmanager
def termination_signals() -> Iterator[None]:
    def terminate(number: int, _frame: FrameType | None) -> NoReturn:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        raise SystemExit(128 + number)

    previous = signal.signal(signal.SIGTERM, terminate)
    try:
        yield
    finally:
        signal.signal(signal.SIGTERM, previous)


def group_members(leader: subprocess.Popen[Any]) -> list[psutil.Process]:
    members = []
    for process in psutil.process_iter():
        with suppress(ProcessLookupError, psutil.NoSuchProcess):
            if (process.pid != leader.pid and os.getpgid(process.pid) == leader.pid
                    and process.status() not in {psutil.STATUS_ZOMBIE, psutil.STATUS_DEAD}):
                members.append(process)
    return members


def kill_group(leader: subprocess.Popen[Any]) -> None:
    while True:
        members = group_members(leader)
        with suppress(ProcessLookupError):
            os.killpg(leader.pid, signal.SIGKILL)
        if not members:
            break
        psutil.wait_procs(members)
    leader.wait()


@contextmanager
def scoped_process(command: list[str], **options: Any) -> Iterator[subprocess.Popen[Any]]:
    process = subprocess.Popen(command, start_new_session=True, **options)
    try:
        yield process
    finally:
        try:
            kill_group(process)
        finally:
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()


@contextmanager
def exit_event(process: subprocess.Popen[Any]) -> Iterator[int]:
    descriptor = os.pidfd_open(process.pid)
    try:
        yield descriptor
    finally:
        os.close(descriptor)


def first_exit(*processes: subprocess.Popen[Any]) -> subprocess.Popen[Any]:
    with ExitStack() as scope:
        exited = {scope.enter_context(exit_event(process)): process for process in processes}
        readable, _, _ = select.select(list(exited), [], [])
        return exited[readable[0]]


def run_checked(command: list[str], **options: Any) -> subprocess.CompletedProcess[Any]:
    finished = subprocess.run(command, **options)
    if finished.returncode != 0:
        output = "\n".join(str(stream) for stream in (finished.stdout, finished.stderr) if stream)
        raise CommandFailed(command, finished.returncode, output)
    return finished
