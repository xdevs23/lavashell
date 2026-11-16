# SPDX-License-Identifier: GPL-3.0-only

from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import ExitStack, contextmanager
from dataclasses import dataclass
import io
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile
from typing import IO, Any

from inotify_simple import INotify, flags
from PIL import Image, ImageChops

from next_steps import NextStep, ToolError
from process_scope import exit_event, piped, scoped_process

HIDDEN_ENVIRONMENT = frozenset({"DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK"})


class VerificationError(ToolError):
    pass


class MissingTool(VerificationError):
    def __init__(self, tool: str) -> None:
        super().__init__(f"{tool} is not available", NextStep.ENTER_NATIVE_SHELL)


class CompositorExited(VerificationError):
    def __init__(self) -> None:
        super().__init__("the private compositor ended before it created its sockets", NextStep.READ_RUN_LOGS)


class AppExited(VerificationError):
    def __init__(self, before: str) -> None:
        super().__init__(f"the app ended before {before}", NextStep.READ_RUN_LOGS)


class WindowEventsEnded(VerificationError):
    def __init__(self) -> None:
        super().__init__("sway ended its window events", NextStep.READ_RUN_LOGS)


class SwayCommandFailed(VerificationError):
    def __init__(self, arguments: tuple[str, ...], results: list[CommandResult]) -> None:
        super().__init__(f"sway refused the command {arguments}: {results}", NextStep.READ_RUN_LOGS)


class SwayRequestFailed(VerificationError):
    def __init__(self, request: tuple[str, ...], exit_code: int, stderr: str) -> None:
        super().__init__(f"swaymsg {request} exited {exit_code} without a reply: {stderr}", NextStep.READ_RUN_LOGS)


class UnreadableSwayReply(VerificationError):
    def __init__(self, request: tuple[str, ...], document: object) -> None:
        super().__init__(f"sway answered {request} with {document!r}", NextStep.REPORT_DEFECT)


@dataclass(frozen=True)
class CommandResult:
    success: bool
    error: str | None


@dataclass(frozen=True)
class Size:
    width: int
    height: int

    @property
    def center(self) -> tuple[int, int]:
        return self.width // 2, self.height // 2


@dataclass(frozen=True)
class SwayWindow:
    app_id: str
    pid: int


@dataclass(frozen=True)
class SwayNode:
    window: SwayWindow | None
    children: tuple[SwayNode, ...]

    def windows(self) -> Iterator[SwayWindow]:
        if self.window is not None:
            yield self.window
        for child in self.children:
            yield from child.windows()


def read_command_result(document: object) -> CommandResult | None:
    match document:
        case {"success": bool(success), "error": str(error)}:
            return CommandResult(success, error)
        case {"success": bool(success)}:
            return CommandResult(success, None)
        case _:
            return None


def read_output_size(document: object) -> Size | None:
    match document:
        case {"rect": {"width": int(width), "height": int(height)}}:
            return Size(width=width, height=height)
        case _:
            return None


def read_window(document: object) -> SwayWindow | None:
    match document:
        case {"app_id": str(app_id), "pid": int(pid)}:
            return SwayWindow(app_id=app_id, pid=pid)
        case _:
            return None


def read_node(document: object) -> SwayNode | None:
    match document:
        case {"nodes": list(nodes), "floating_nodes": list(floating_nodes)}:
            children = [read_node(child) for child in [*nodes, *floating_nodes]]
            if None in children:
                return None
            return SwayNode(window=read_window(document), children=tuple(filter(None, children)))
        case _:
            return None


@dataclass(frozen=True)
class Tools:
    sway: str
    swaymsg: str
    grim: str

    @classmethod
    def find(cls) -> Tools:
        return cls(sway=find_tool("sway"), swaymsg=find_tool("swaymsg"), grim=find_tool("grim"))


def find_tool(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise MissingTool(name)
    return path


@dataclass(frozen=True)
class HeadlessSway:
    runtime: Path
    environment: dict[str, str]
    display: str
    tools: Tools

    def reply[Model](self, request: tuple[str, ...], read: Callable[[object], Model | None]) -> Model:
        answer = subprocess.run(
            [self.tools.swaymsg, *request], env=self.environment, capture_output=True, text=True,
        )
        if not answer.stdout:
            raise SwayRequestFailed(request, answer.returncode, answer.stderr)
        try:
            document: object = json.loads(answer.stdout)
        except json.JSONDecodeError as error:
            raise UnreadableSwayReply(request, answer.stdout) from error
        if (model := read(document)) is None:
            raise UnreadableSwayReply(request, document)
        return model

    def replies[Model](self, request: tuple[str, ...], read: Callable[[object], Model | None]) -> list[Model]:
        def read_all(document: object) -> list[Model] | None:
            match document:
                case list(items) if None not in (models := [read(item) for item in items]):
                    return [model for model in models if model is not None]
                case _:
                    return None

        return self.reply(request, read_all)

    def command(self, *arguments: str) -> None:
        results = self.replies(arguments, read_command_result)
        if not all(result.success for result in results):
            raise SwayCommandFailed(arguments, results)

    def output_size(self) -> Size:
        [size] = self.replies(("-t", "get_outputs"), read_output_size)
        return size

    def windows(self) -> list[SwayWindow]:
        return list(self.reply(("-t", "get_tree"), read_node).windows())

    def wait_for_windows(
        self, ready: Callable[[list[SwayWindow]], bool], client: subprocess.Popen[Any] | None = None,
    ) -> list[SwayWindow]:
        subscription = [self.tools.swaymsg, "-t", "subscribe", "-m", json.dumps(["tick", "window"])]
        with ExitStack() as scope:
            events = scope.enter_context(
                scoped_process(subscription, env=self.environment, stdout=subprocess.PIPE),
            )
            events_pipe = piped(events.stdout).fileno()
            watched = [events_pipe]
            client_exited = None if client is None else scope.enter_context(exit_event(client))
            if client_exited is not None:
                watched.append(client_exited)
            while True:
                readable, _, _ = select.select(watched, [], [])
                if client_exited in readable:
                    raise AppExited("its windows were ready")
                if not os.read(events_pipe, io.DEFAULT_BUFFER_SIZE):
                    raise WindowEventsEnded()
                windows = self.windows()
                if ready(windows):
                    return windows

    def capture(self, path: Path) -> Path:
        subprocess.run([self.tools.grim, str(path)], env=self.environment, check=True)
        return path

    def capture_until_changed(self, path: Path, reference: Path, client: subprocess.Popen[Any]) -> Path:
        with Image.open(reference) as before:
            while True:
                if client.poll() is not None:
                    raise AppExited("its output changed")
                with Image.open(self.capture(path)) as after:
                    if ImageChops.difference(before.convert("RGB"), after.convert("RGB")).getbbox():
                        return path


def ipc_sockets(runtime: Path) -> list[Path]:
    return sorted(runtime.glob("sway-ipc.*.sock"))


def display_sockets(runtime: Path) -> list[Path]:
    return sorted(path for path in runtime.glob("wayland-*") if not path.name.endswith(".lock"))


def wait_for_sockets(runtime: Path, compositor: subprocess.Popen[Any]) -> tuple[Path, str]:
    with INotify() as watch, exit_event(compositor) as compositor_exited:
        watch.add_watch(runtime, flags.CREATE)
        while not (ipc_sockets(runtime) and display_sockets(runtime)):
            readable, _, _ = select.select([watch, compositor_exited], [], [])
            if compositor_exited in readable:
                raise CompositorExited()
            watch.read()
    [ipc] = ipc_sockets(runtime)
    [display] = display_sockets(runtime)
    return ipc, display.name


@contextmanager
def headless_sway(*, drm_device: str | None, log: IO[str]) -> Iterator[HeadlessSway]:
    tools = Tools.find()
    with tempfile.TemporaryDirectory(prefix="headless-sway-") as directory:
        runtime = Path(directory)
        config = runtime / "sway.conf"
        config.write_text(
            "default_border none\ndefault_floating_border none\n"
            "xwayland disable\nseat seat0 fallback true\n"
        )
        environment = {key: value for key, value in os.environ.items() if key not in HIDDEN_ENVIRONMENT}
        environment.update(
            XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
            WLR_LIBINPUT_NO_DEVICES="1", WLR_RENDERER="gles2",
        )
        if drm_device:
            environment["WLR_RENDER_DRM_DEVICE"] = drm_device
        with scoped_process(
            [tools.sway, "--unsupported-gpu", "--config", str(config)],
            env=environment, stdout=log, stderr=subprocess.STDOUT,
        ) as compositor:
            ipc_socket, display = wait_for_sockets(runtime, compositor)
            environment["SWAYSOCK"] = str(ipc_socket)
            environment["WAYLAND_DISPLAY"] = display
            yield HeadlessSway(runtime, environment, display, tools)
