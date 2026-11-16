# SPDX-License-Identifier: GPL-3.0-only

import enum
import os
from types import TracebackType
from typing import NamedTuple, Self

class Event(NamedTuple):
    wd: int
    mask: int
    cookie: int
    name: str

class flags(enum.IntEnum):
    CREATE = ...

class INotify:
    def __init__(self, inheritable: bool = False, nonblocking: bool = False, closefd: bool = True) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, error_type: type[BaseException] | None, error: BaseException | None, traceback: TracebackType | None,
    ) -> None: ...
    def fileno(self) -> int: ...
    def add_watch(self, path: str | os.PathLike[str], mask: int) -> int: ...
    def read(self, timeout: int | None = None, read_delay: int | None = None) -> list[Event]: ...
