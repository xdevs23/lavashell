#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

from __future__ import annotations

import argparse
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
from typing import IO, Any

from next_steps import NextStep, ToolError, exit_with_next_step
from process_scope import CommandFailed, first_exit, run_checked, scoped_process, termination_signals

LOOPBACK = "127.0.0.1"
INSTRUMENTATION_STATUS = ("INSTRUMENTATION_RESULT:", "INSTRUMENTATION_FAILED:", "INSTRUMENTATION_CODE:")
USER_UNLOCKED_EVENT = "uc_finish_user_unlocking"

type JsonValue = str | int | float | bool | None | list[JsonValue] | dict[str, JsonValue]


class EmulatorFailure(ToolError):
    pass


class EmulatorExited(EmulatorFailure):
    def __init__(self) -> None:
        super().__init__("the emulator ended before the device finished booting", NextStep.READ_RUN_LOGS)


class AdbServerExited(EmulatorFailure):
    def __init__(self) -> None:
        super().__init__("the adb server ended before the device finished booting", NextStep.READ_RUN_LOGS)


class InstrumentationCrashed(EmulatorFailure):
    def __init__(self, output: str) -> None:
        super().__init__(f"the instrumentation ended without round trip results:\n{output}", NextStep.READ_RUN_LOGS)


class RoundTripsMissing(EmulatorFailure):
    def __init__(self) -> None:
        super().__init__("the instrumentation ended without printing a round trip result", NextStep.READ_RUN_LOGS)


class ResultMalformed(EmulatorFailure):
    def __init__(self, line: str) -> None:
        super().__init__(
            f"the instrumentation printed a line that is not a round trip result: {line!r}", NextStep.REPORT_DEFECT,
        )


class RoundTripsFailed(EmulatorFailure):
    def __init__(self, steps: list[str]) -> None:
        super().__init__(
            f"the pane showed another color or size, or the round trips failed, in the steps {steps}",
            NextStep.READ_RUN_LOGS,
        )


@dataclass(frozen=True)
class Settings:
    apk: Path
    signer: Path
    sdk: Path
    build_tools: str
    emulator_sdk: Path
    api: int
    abi: str
    page_size: int
    alignment: int
    runner: str
    gpu: str
    output: Path | None
    work: Path | None

    @classmethod
    def read(cls) -> Settings:
        parser = argparse.ArgumentParser()
        parser.add_argument("--apk", type=Path, required=True, help="Unsigned package of the round trips")
        parser.add_argument("--signer", type=Path, required=True, help="Script that signs APKs")
        parser.add_argument("--sdk", type=Path, required=True, help="SDK with the build tools")
        parser.add_argument("--build-tools", required=True, help="Version of the build tools")
        parser.add_argument("--emulator-sdk", type=Path, required=True, help="SDK with the emulator")
        parser.add_argument("--api", type=int, required=True, help="API level of the system image")
        parser.add_argument("--abi", required=True, help="ABI of the system image")
        parser.add_argument("--page-size", type=int, required=True, help="Page size in bytes of the package alignment")
        parser.add_argument(
            "--alignment", type=int, required=True, help="Alignment in bytes of the files in the package",
        )
        parser.add_argument("--runner", required=True, help="Component name of the instrumentation runner")
        parser.add_argument("--gpu", default="swiftshader_indirect", help="GPU mode of the emulator")
        parser.add_argument("--output", type=Path, help="Write the results as JSON lines")
        parser.add_argument("--work", type=Path, help="Directory for the data of the emulator, about 8 GB")
        return cls(**vars(parser.parse_args()))


@dataclass(frozen=True)
class RoundTripResult:
    step: str
    matches: bool | None
    evidence: dict[str, JsonValue]

    @classmethod
    def read(cls, line: str) -> RoundTripResult:
        try:
            return cls(**json.loads(line))
        except (json.JSONDecodeError, TypeError) as error:
            raise ResultMalformed(line) from error


def run_converting_to_strings(*parts: str | Path | int, **options: Any) -> subprocess.CompletedProcess[Any]:
    return run_checked([str(part) for part in parts], **options)


def write_ini(path: Path, values: Mapping[str, str | Path]) -> None:
    path.write_text("".join(f"{key}={value}\n" for key, value in values.items()))


def device(work: Path, settings: Settings) -> dict[str, str]:
    api = settings.api
    avd = work / "avd"
    (avd / "probe.avd").mkdir(parents=True)
    write_ini(avd / "probe.ini", {"path": avd / "probe.avd", "target": f"android-{api}"})
    write_ini(avd / "probe.avd/config.ini", {
        "abi.type": settings.abi,
        "hw.cpu.arch": settings.abi,
        "image.sysdir.1": f"system-images/android-{api}/default/{settings.abi}/",
        "tag.id": "default",
        "hw.gpu.enabled": "yes",
        "hw.gpu.mode": settings.gpu,
    })
    home = work / "home"
    home.mkdir()
    sdk = settings.emulator_sdk
    return dict(
        os.environ, ANDROID_SDK_ROOT=str(sdk), ANDROID_HOME=str(sdk), ANDROID_AVD_HOME=str(avd),
        ANDROID_EMULATOR_HOME=str(home), ANDROID_USER_HOME=str(home), TMPDIR=str(work),
        ADB_USB="0", ADB_MDNS="0", ADB_EMU="0",
    )


@dataclass(frozen=True)
class AdbServer:
    process: subprocess.Popen[Any]
    client: list[str]
    environment: dict[str, str]


@contextmanager
def adb_server(program: Path, environment: dict[str, str], log: IO[str]) -> Iterator[AdbServer]:
    with socket.socket() as listener:
        listener.bind((LOOPBACK, 0))
        listener.listen()
        port = listener.getsockname()[1]
        server_environment = {**environment, "ANDROID_ADB_SERVER_PORT": str(port)}
        command = [str(program), "-L", f"acceptfd:{listener.fileno()}", "server", "nodaemon"]
        with scoped_process(
            command, env=server_environment, stdout=log, stderr=subprocess.STDOUT, pass_fds=[listener.fileno()],
        ) as server:
            yield AdbServer(process=server, client=[str(program), "-P", str(port)], environment=server_environment)


def wait_for_boot(adb: AdbServer, emulator: subprocess.Popen[Any]) -> None:
    online = [*adb.client, "wait-for-device"]
    user_unlocked = [*adb.client, "shell", "logcat", "-b", "events", "-m", "1", "-s", USER_UNLOCKED_EVENT]
    for command in (online, user_unlocked):
        with scoped_process(command, env=adb.environment, stdout=subprocess.DEVNULL) as waiting:
            exited = first_exit(emulator, adb.process, waiting)
            if exited is emulator:
                raise EmulatorExited()
            if exited is adb.process:
                raise AdbServerExited()
            if waiting.wait() != 0:
                raise CommandFailed(command, waiting.returncode)


def read_results(target: list[str], environment: dict[str, str], runner: str) -> list[RoundTripResult]:
    command = [*target, "shell", "am", "instrument", "-w", runner]
    instrument = run_checked(command, env=environment, capture_output=True, text=True)
    lines = [line for line in instrument.stdout.splitlines() if line.strip()]
    if any(line.startswith(INSTRUMENTATION_STATUS) for line in lines):
        raise InstrumentationCrashed(instrument.stdout)
    results = [RoundTripResult.read(line) for line in lines]
    if not results:
        raise RoundTripsMissing()
    return results


@contextmanager
def device_log_saved(target: list[str], environment: dict[str, str], output: Path | None) -> Iterator[None]:
    try:
        yield
    finally:
        if output:
            command = [*target, "logcat", "-d"]
            device_log = subprocess.run(command, env=environment, capture_output=True, text=True)
            if device_log.returncode == 0:
                output.with_suffix(".logcat").write_text(device_log.stdout)
            else:
                print(CommandFailed(command, device_log.returncode, device_log.stderr), file=sys.stderr)


def emulate(settings: Settings, work: Path, log: IO[str]) -> list[RoundTripResult]:
    apk = work / "probe.apk"
    run_converting_to_strings(
        sys.executable, settings.signer, "--sdk", settings.sdk, "--build-tools", settings.build_tools,
        "--page-size", settings.page_size, "--alignment", settings.alignment, "--apk", settings.apk,
        "--output", apk,
    )
    command = [
        str(settings.emulator_sdk / "emulator/emulator"), "-avd", "probe", "-no-window", "-no-audio",
        "-no-boot-anim", "-no-snapshot", "-no-metrics", "-accel", "on", "-gpu", settings.gpu,
        "-feature", "GuestAngle",
    ]
    with (
        adb_server(settings.emulator_sdk / "platform-tools/adb", device(work, settings), log) as adb,
        scoped_process(command, env=adb.environment, stdout=log, stderr=subprocess.STDOUT) as emulator,
    ):
        wait_for_boot(adb, emulator)
        run_converting_to_strings(*adb.client, "install", "-r", apk, env=adb.environment)
        with device_log_saved(adb.client, adb.environment, settings.output):
            return read_results(adb.client, adb.environment, settings.runner)


def round_trips(settings: Settings) -> list[RoundTripResult]:
    with (
        termination_signals(),
        tempfile.TemporaryDirectory(prefix="lavashell-emulator-", dir=settings.work) as work,
        tempfile.NamedTemporaryFile("w", prefix="lavashell-emulator-", suffix=".log", delete=False) as log,
    ):
        try:
            results = emulate(settings, Path(work), log)
        except BaseException:
            print(f"the log of the emulator and the adb server is {log.name}", file=sys.stderr)
            raise
    Path(log.name).unlink()
    return results


def main() -> None:
    settings = Settings.read()
    try:
        results = round_trips(settings)
        lines = [json.dumps(asdict(result)) for result in results]
        print("\n".join(lines), flush=True)
        if settings.output:
            settings.output.write_text("".join(line + "\n" for line in lines))
        if failed := [result.step for result in results if result.matches is False]:
            raise RoundTripsFailed(failed)
    except ToolError as failure:
        exit_with_next_step(failure)


if __name__ == "__main__":
    main()
