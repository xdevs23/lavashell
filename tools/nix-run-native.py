# SPDX-License-Identifier: GPL-3.0-only

from __future__ import annotations

import argparse
from collections.abc import Iterator
from contextlib import AbstractContextManager
from dataclasses import asdict, dataclass
import enum
import json
from pathlib import Path
import select
import subprocess
import tempfile
import time
from typing import IO

import psutil

from headless_sway import AppExited, HeadlessSway, SwayWindow, VerificationError, find_tool, headless_sway
from next_steps import NextStep, exit_with_next_step
from process_scope import exit_event, piped, scoped_process, termination_signals
from repository_root import REPOSITORY_ROOT

# https://gitlab.freedesktop.org/xkeyboard-config/xkeyboard-config/-/blob/master/keycodes/evdev
SPACE_BAR = "SPCE"
LABEL_SEPARATORS = "-_"
APP_DECLARATIONS = REPOSITORY_ROOT / "nix/app-declarations.json"


class AppDeclarationValue(enum.Enum):
    @staticmethod
    def _generate_next_value_(name: str, start: int, count: int, last_values: list[object]) -> str:
        return name.lower()


class Host(AppDeclarationValue):
    AOSP = enum.auto()
    CHECKOUT = enum.auto()
    WAYLAND = enum.auto()


class LaunchKind(AppDeclarationValue):
    HELPER = enum.auto()
    WINDOW = enum.auto()


class InvalidDeclaration(VerificationError):
    def __init__(self, app: str, problem: str) -> None:
        super().__init__(f"the declaration of {app} is invalid: {problem}", NextStep.FIX_APP_DECLARATION)


class UnreadableSpellings(VerificationError):
    def __init__(self, path: Path) -> None:
        super().__init__(f"{path} does not list the hosts and launch kinds as text", NextStep.REPORT_DEFECT)


class DivergedSpellings(VerificationError):
    def __init__(self, values: type[AppDeclarationValue], spellings: list[str]) -> None:
        super().__init__(
            f"{APP_DECLARATIONS.name} lists {', '.join(spellings)} as the values of {values.__name__}, "
            f"which are {', '.join(str(value.value) for value in values)}", NextStep.REPORT_DEFECT,
        )


class UnknownApps(VerificationError):
    def __init__(self, names: list[str]) -> None:
        super().__init__(
            f"--only requires packaged Wayland apps, but these are not: {', '.join(names)}", NextStep.FIX_ARGUMENTS,
        )


class PackageNotBuilt(VerificationError):
    def __init__(self, app: str) -> None:
        super().__init__(f"the package of {app} is not built", NextStep.BUILD_PACKAGE)


class UnexpectedExecutable(VerificationError):
    def __init__(self, app: str, executable: str) -> None:
        super().__init__(f"{app} launched the unexpected executable {executable}", NextStep.READ_RUN_LOGS)


class PathNotEmpty(VerificationError):
    def __init__(self, app: str, path: str | None) -> None:
        super().__init__(f"{app} changed its empty runtime PATH to {path!r}", NextStep.READ_RUN_LOGS)


class AppFailed(VerificationError):
    def __init__(self, app: str, exit_code: int) -> None:
        super().__init__(f"{app} exited {exit_code}", NextStep.READ_RUN_LOGS)


class HelperNotReady(VerificationError):
    def __init__(self, line: str) -> None:
        super().__init__(f"the helper did not become ready, it wrote {line!r}", NextStep.READ_RUN_LOGS)


class UnreadableReply(VerificationError):
    def __init__(self, line: str) -> None:
        super().__init__(f"the helper wrote a line that is not a reply: {line!r}", NextStep.REPORT_DEFECT)


class HelperFailed(VerificationError):
    def __init__(self, replies: list[Reply]) -> None:
        super().__init__(f"the helper did not finish every command, its replies were {replies}", NextStep.READ_RUN_LOGS)


class UnexpectedWindows(VerificationError):
    def __init__(self, windows: list[SwayWindow]) -> None:
        super().__init__(f"the app opened {len(windows)} windows instead of one: {windows}", NextStep.READ_RUN_LOGS)


@dataclass(frozen=True)
class Move:
    x: int
    y: int

    def line(self) -> str:
        return json.dumps({"command": "move", **asdict(self)})


@dataclass(frozen=True)
class Key:
    name: str

    def line(self) -> str:
        return json.dumps({"command": "key", **asdict(self)})


@dataclass(frozen=True)
class Ready:
    pid: int


@dataclass(frozen=True)
class Done:
    pass


@dataclass(frozen=True)
class Failed:
    message: str


type Reply = Ready | Done | Failed


def read_reply(line: str) -> Reply:
    try:
        document = json.loads(line)
    except json.JSONDecodeError as error:
        raise UnreadableReply(line) from error
    match document:
        case {"reply": "ready", "pid": int(pid)}:
            return Ready(pid=pid)
        case {"reply": "done"}:
            return Done()
        case {"reply": "failed", "message": str(message)}:
            return Failed(message=message)
        case _:
            raise UnreadableReply(line)


@dataclass(frozen=True)
class ClientProcess:
    command: list[str]
    directory: Path
    environment: dict[str, str]
    log: IO[str]


@dataclass(frozen=True)
class Run:
    client: subprocess.Popen[str]
    sway: HeadlessSway
    screenshot: Path
    empty_output: Path


@dataclass(frozen=True)
class WindowLaunch:
    arguments: tuple[str, ...]

    def spawn(self, process: ClientProcess) -> AbstractContextManager[subprocess.Popen[str]]:
        return scoped_process(
            process.command, cwd=process.directory, env=process.environment, stdin=subprocess.PIPE,
            stdout=process.log, stderr=process.log, text=True,
        )

    def start(self, client: subprocess.Popen[str], sway: HeadlessSway) -> psutil.Process:
        match sway.wait_for_windows(bool, client):
            case [window]:
                return psutil.Process(window.pid)
            case windows:
                raise UnexpectedWindows(windows)

    def finish(self, run: Run) -> Path:
        run.sway.capture_until_changed(run.screenshot, run.empty_output, run.client)
        run.sway.command('[app_id=".*"]', "kill")
        run.client.wait()
        return run.screenshot


@dataclass(frozen=True)
class HelperLaunch:
    arguments: tuple[str, ...]

    def spawn(self, process: ClientProcess) -> AbstractContextManager[subprocess.Popen[str]]:
        return scoped_process(
            process.command, cwd=process.directory, env=process.environment, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=process.log, text=True,
        )

    def start(self, client: subprocess.Popen[str], sway: HeadlessSway) -> psutil.Process:
        helper_output = piped(client.stdout)
        with exit_event(client) as client_exited:
            readable, _, _ = select.select([helper_output, client_exited], [], [])
        if helper_output not in readable or not (line := helper_output.readline()):
            raise AppExited("it became ready")
        match read_reply(line):
            case Ready(pid=pid):
                return psutil.Process(pid)
            case _:
                raise HelperNotReady(line)

    def finish(self, run: Run) -> None:
        requests: list[Move | Key] = [Move(*run.sway.output_size().center), Key(SPACE_BAR)]
        try:
            with piped(run.client.stdin) as helper_input:
                helper_input.write("".join(request.line() + "\n" for request in requests))
        except BrokenPipeError as error:
            raise AppExited("it read every command") from error
        replies = [read_reply(line) for line in piped(run.client.stdout).read().splitlines()]
        run.client.wait()
        if replies != [Done()] * len(requests):
            raise HelperFailed(replies)


type Launch = WindowLaunch | HelperLaunch


@dataclass(frozen=True)
class WaylandApp:
    name: str
    program: Path
    launch: Launch


@dataclass(frozen=True)
class Settings:
    nix: list[str]
    flake: str
    label: str
    output: Path
    home: Path
    sway: HeadlessSway
    empty_output: Path


@dataclass(frozen=True)
class AppResult:
    name: str
    exit_code: int
    seconds: float
    screenshot: str | None
    executable: str


@dataclass(frozen=True)
class Report:
    flake: str
    all_wayland_apps: bool
    examples: list[AppResult]


def check_declared_spellings(path: Path) -> None:
    match json.loads(path.read_text()):
        case {"hosts": list(hosts), "launchKinds": list(launch_kinds)} if all(
            isinstance(spelling, str) for spelling in [*hosts, *launch_kinds]
        ):
            declared: list[tuple[type[AppDeclarationValue], list[str]]] = [(Host, hosts), (LaunchKind, launch_kinds)]
        case _:
            raise UnreadableSpellings(path)
    for values, spellings in declared:
        if sorted(spellings) != sorted(str(value.value) for value in values):
            raise DivergedSpellings(values, spellings)


def read_closed_set[Value: AppDeclarationValue](app: str, values: type[Value], text: object) -> Value:
    try:
        return values(text)
    except ValueError as error:
        spellings = ", ".join(str(value.value) for value in values)
        raise InvalidDeclaration(app, f"{text!r} is not one of {spellings}") from error


def read_launch(app: str, meta: object) -> Launch:
    match meta:
        case {"lavashellLaunch": {"kind": kind, "arguments": list(arguments)}} if all(
            isinstance(argument, str) for argument in arguments
        ):
            launch_arguments = tuple(arguments)
        case _:
            raise InvalidDeclaration(app, "a Wayland app needs a launch with a kind and a list of text arguments")
    match read_closed_set(app, LaunchKind, kind):
        case LaunchKind.WINDOW:
            return WindowLaunch(launch_arguments)
        case LaunchKind.HELPER:
            return HelperLaunch(launch_arguments)


def read_wayland_app(name: str, declaration: object) -> WaylandApp | None:
    match declaration:
        case {"program": str(program), "meta": {"lavashellHost": host} as meta}:
            if read_closed_set(name, Host, host) is not Host.WAYLAND:
                return None
            return WaylandApp(name=name, program=Path(program), launch=read_launch(name, meta))
        case _:
            raise InvalidDeclaration(name, "an app needs a program and a host")


def current_system(nix: list[str]) -> str:
    return subprocess.check_output(
        [*nix, "eval", "--offline", "--impure", "--raw", "--expr", "builtins.currentSystem"], text=True,
    )


def wayland_apps(nix: list[str], flake: str, only: list[str]) -> list[WaylandApp]:
    check_declared_spellings(APP_DECLARATIONS)
    declarations = json.loads(subprocess.check_output(
        [*nix, "eval", "--offline", "--json", f"{flake}#apps.{current_system(nix)}"], text=True,
    ))
    apps = [
        app for name, declaration in sorted(declarations.items())
        if (app := read_wayland_app(name, declaration)) is not None
    ]
    if unknown := sorted(set(only) - {app.name for app in apps}):
        raise UnknownApps(unknown)
    return [app for app in apps if not only or app.name in only]


def wrapped_executable(app: WaylandApp) -> Path:
    if not app.program.is_file():
        raise PackageNotBuilt(app.name)
    return app.program.with_name(f".{app.program.name}-wrapped").resolve(strict=True)


def run_app(app: WaylandApp, settings: Settings) -> AppResult:
    executable = wrapped_executable(app)
    app_home = settings.home / app.name
    app_home.mkdir()
    started = time.monotonic()
    with (settings.output / f"{settings.label}-{app.name}.log").open("w") as client_log:
        process = ClientProcess(
            command=[*settings.nix, "run", "--offline", f"{settings.flake}#{app.name}", "--", *app.launch.arguments],
            directory=app_home,
            environment={
                "PATH": "", "HOME": str(app_home), "LANG": "C.UTF-8",
                "XDG_RUNTIME_DIR": str(settings.sway.runtime), "WAYLAND_DISPLAY": settings.sway.display,
            },
            log=client_log,
        )
        with app.launch.spawn(process) as client:
            running = app.launch.start(client, settings.sway)
            if Path(running.exe()) != executable:
                raise UnexpectedExecutable(app.name, running.exe())
            if (path := running.environ().get("PATH")) != "":
                raise PathNotEmpty(app.name, path)
            screenshot = app.launch.finish(Run(
                client, settings.sway, settings.output / f"{settings.label}-{app.name}.png", settings.empty_output,
            ))
            if client.returncode != 0:
                raise AppFailed(app.name, client.returncode)
    return AppResult(
        name=app.name, exit_code=client.returncode, seconds=round(time.monotonic() - started, 3),
        screenshot=screenshot.name if screenshot else None, executable=str(executable),
    )


def label_argument(text: str) -> str:
    if not text or not text.isascii() or not all(c.isalnum() or c in LABEL_SEPARATORS for c in text):
        raise argparse.ArgumentTypeError(f"{text!r} must be ASCII letters, digits, '-' and '_'")
    return text


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--flake")
    parser.add_argument("--label", type=label_argument, default="nix-run-native")
    parser.add_argument("--only", action="append", default=[], help="limit this run to named apps")
    parser.add_argument("--drm-device", help="render node of the compositor (default: chosen by the compositor)")
    parser.add_argument("--output-dir", type=Path, help="directory of the logs, screenshots and report "
                        "(default: a new temporary directory)")
    return parser.parse_args()


def results_of(apps: list[WaylandApp], settings: Settings) -> Iterator[AppResult]:
    for app in apps:
        settings.sway.wait_for_windows(lambda windows: not windows)
        yield run_app(app, settings)
        print(f"Passed nix run #{app.name} with empty PATH", flush=True)


def main() -> None:
    args = parse_arguments()
    nix = [find_tool("nix"), "--extra-experimental-features", "nix-command flakes"]
    flake = args.flake or f"git+{REPOSITORY_ROOT.as_uri()}"
    output = args.output_dir or Path(tempfile.mkdtemp(prefix="nix-run-native-"))
    output.mkdir(parents=True, exist_ok=True)
    apps = wayland_apps(nix, flake, args.only)
    compositor_log = output / f"{args.label}-compositor.log"
    with tempfile.TemporaryDirectory(prefix="nix-run-") as home, compositor_log.open("w") as log:
        with headless_sway(drm_device=args.drm_device, log=log) as sway:
            settings = Settings(
                nix=nix, flake=flake, label=args.label, output=output, home=Path(home), sway=sway,
                empty_output=sway.capture(output / f"{args.label}-empty.png"),
            )
            report = Report(flake=flake, all_wayland_apps=not args.only, examples=list(results_of(apps, settings)))
    report_path = output / f"{args.label}.json"
    report_path.write_text(json.dumps(asdict(report), indent=2) + "\n")
    print(f"Verified {len(apps)} nix run apps; report {report_path}", flush=True)


if __name__ == "__main__":
    with termination_signals():
        try:
            main()
        except VerificationError as error:
            exit_with_next_step(error)
