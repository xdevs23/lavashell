#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run the round trips of SurfaceStream in a headless AOSP emulator and print the results.

Run it with: nix run .#surface-stream-round-trip
The emulator runs with KVM and without a window. It gets its own adb server, its own ports and a
data directory that is deleted afterwards. The adb server ignores USB devices and other emulators.
The instrumentation of the round trips prints one JSON object per step. A step whose object has
`matches` set to false fails the run.
"""

import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

from process_scope import first_exit, scoped_process, termination_signals

# The class of the instrumentation in the package of the round trips.
RUNNER = ".StreamRoundTripRunner"
# The console ports offered to emulators, each followed by the adb port of that emulator.
CONSOLE_PORTS = range(5554, 5682, 2)


def run(*command, **options):
    return subprocess.run([str(part) for part in command], check=True, **options)


def free(port):
    with socket.socket() as probe:
        try:
            probe.bind(("127.0.0.1", port))
        except OSError:
            return False
    return True


def ports():
    """The console port and the adb port used by the emulator, and a port for the adb server."""
    console = next(port for port in CONSOLE_PORTS if free(port) and free(port + 1))
    with socket.socket() as server:
        server.bind(("127.0.0.1", 0))
        return console, server.getsockname()[1]


def write_ini(path, values):
    path.write_text("".join(f"{key}={value}\n" for key, value in values.items()))


def device(work, sdk, api, gpu):
    """Creates the virtual device in `work` and returns the environment variables for running the emulator on it."""
    avd = work / "avd"
    (avd / "probe.avd").mkdir(parents=True)
    write_ini(avd / "probe.ini", {"path": avd / "probe.avd", "target": f"android-{api}"})
    write_ini(avd / "probe.avd/config.ini", {
        "abi.type": "x86_64",
        "hw.cpu.arch": "x86_64",
        "image.sysdir.1": f"system-images/android-{api}/default/x86_64/",
        "tag.id": "default",
        "hw.gpu.enabled": "yes",
        "hw.gpu.mode": gpu,
    })
    home = work / "home"
    home.mkdir()
    return dict(
        os.environ, ANDROID_SDK_ROOT=str(sdk), ANDROID_HOME=str(sdk), ANDROID_AVD_HOME=str(avd),
        ANDROID_EMULATOR_HOME=str(home), ANDROID_USER_HOME=str(home), TMPDIR=str(work),
        ADB_USB="0", ADB_MDNS="0", ADB_EMU="0",
    )


def booted(target, environment, emulator):
    """Waits until AOSP has booted, or until the emulator ends and the result is false."""
    # The package manager accepts installs only once sys.boot_completed is set, which comes after the screen is on.
    online = [*target, "wait-for-device"]
    boot_completed = [*target, "shell", "until [ \"$(getprop sys.boot_completed)\" = 1 ]; do sleep 1; done"]
    for command in (online, boot_completed):
        with scoped_process(command, env=environment, stdout=subprocess.DEVNULL) as waiting:
            if first_exit(emulator, waiting) is emulator:
                return False
    return True


def package(build_tools, apk):
    """The package name declared in the manifest of `apk`."""
    dump = run(build_tools / "aapt2", "dump", "packagename", apk, stdout=subprocess.PIPE, text=True)
    return dump.stdout.strip()


def results(target, environment, runner):
    """The results returned by the instrumentation `runner`, one JSON object per step, or None when
    the round trips did not finish. The output of unfinished round trips goes to standard error."""
    instrument = subprocess.run([*target, "shell", "am", "instrument", "-w", runner], env=environment,
                                capture_output=True, text=True)
    try:
        return [json.loads(line) for line in instrument.stdout.splitlines() if line.strip()] or None
    except json.JSONDecodeError:
        print(instrument.stdout, instrument.stderr, sep="\n", file=sys.stderr)
        return None


def emulate(args, work, log):
    """Runs the round trips in an emulator that writes to the open file `log`. The result is the list of steps, or None
    when the emulator did not boot or the round trips did not finish."""
    apk = work / "probe.apk"
    run(sys.executable, args.signer, "--sdk", args.sdk, "--build-tools", args.build_tools,
        "--page-size-kib", args.page_size_kib, "--apk", args.apk, "--output", apk)
    runner = f"{package(args.sdk / 'build-tools' / args.build_tools, apk)}/{RUNNER}"
    console, server = ports()
    environment = device(work, args.emulator_sdk, args.api, args.gpu)
    environment["ANDROID_ADB_SERVER_PORT"] = str(server)
    adb = [str(args.emulator_sdk / "platform-tools/adb"), "-P", str(server)]
    target = [*adb, "-s", f"emulator-{console}"]
    run(*adb, "start-server", env=environment)
    # The GLES translation in the emulator stores a GL copy of each buffer drawn by SurfaceFlinger.
    # Vulkan cannot read that copy when it imports the buffer. GuestAngle runs GL in the guest on top
    # of Vulkan.
    command = [
        args.emulator_sdk / "emulator/emulator", "-avd", "probe", "-port", str(console), "-no-window",
        "-no-audio", "-no-boot-anim", "-no-snapshot", "-no-metrics", "-accel", "on", "-gpu", args.gpu,
        "-feature", "GuestAngle",
    ]
    with scoped_process(command, env=environment, stdout=log, stderr=subprocess.STDOUT) as emulator:
        try:
            if not booted(target, environment, emulator):
                return None
            run(*target, "install", "-r", apk, env=environment)
            found = results(target, environment, runner)
            if args.output:
                device_log = run(*target, "logcat", "-d", env=environment, stdout=subprocess.PIPE, text=True)
                args.output.with_suffix(".logcat").write_text(device_log.stdout)
            return found
        finally:
            subprocess.run([*target, "emu", "kill"], env=environment)
            emulator.wait()
            subprocess.run([*adb, "kill-server"], env=environment)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--apk", type=Path, required=True, help="Unsigned package of the round trips")
    parser.add_argument("--signer", type=Path, required=True, help="Script that signs APKs")
    parser.add_argument("--sdk", type=Path, required=True, help="SDK with the build tools")
    parser.add_argument("--build-tools", required=True, help="Version of the build tools")
    parser.add_argument("--emulator-sdk", type=Path, required=True, help="SDK with the emulator")
    parser.add_argument("--api", type=int, required=True, help="API level of the system image")
    parser.add_argument("--page-size-kib", type=int, required=True, help="Page size that the package aligns to")
    parser.add_argument("--gpu", default="swiftshader_indirect", help="GPU mode of the emulator")
    parser.add_argument("--output", type=Path, help="Write the results as JSON lines")
    parser.add_argument("--work", type=Path, help="Directory for the data of the emulator, about 8 GB")
    args = parser.parse_args()
    # A stopped run deletes the working directory and stops the emulator and the adb server. The log of the
    # emulator remains when the run does not end with results.
    with termination_signals(), tempfile.TemporaryDirectory(prefix="lavashell-emulator-", dir=args.work) as work, \
            tempfile.NamedTemporaryFile("w", prefix="lavashell-emulator-", suffix=".log", delete=False) as log:
        try:
            found = emulate(args, Path(work), log)
        except BaseException:
            print(f"the log of the emulator is {log.name}", file=sys.stderr)
            raise
    if not found:
        sys.exit(f"the emulator did not boot, or the round trips did not finish; the log of the emulator is {log.name}")
    Path(log.name).unlink()
    lines = [json.dumps(result) for result in found]
    print("\n".join(lines), flush=True)
    if args.output:
        args.output.write_text("".join(line + "\n" for line in lines))
    failed = [result.get("step") for result in found if result.get("matches") is False]
    if failed:
        sys.exit(f"the pane showed another color or size, or the round trips failed, in the steps {failed}")


if __name__ == "__main__":
    main()
