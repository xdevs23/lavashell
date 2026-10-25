# SPDX-License-Identifier: GPL-3.0-only
"""Launch each packaged Wayland app of the flake with `nix run` in a headless sway session.

Each app runs with an empty PATH and a new home directory, so it can use only the programs and assets declared by its
package.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

import psutil

from headless_sway import headless_sway
from process_scope import scoped_process, termination_signals

SPACE_BAR = "SPCE"


def center_of(rectangle):
    return rectangle["width"] // 2, rectangle["height"] // 2


class WindowLaunch:
    """An app that maps a window, draws into it and exits once the window is closed."""

    piped = False

    def __init__(self, declaration):
        self.arguments = declaration.get("arguments", [])

    def start(self, client, sway):
        [window, *_] = sway.wait_for_windows(bool, client)
        return psutil.Process(window["pid"])

    def finish(self, client, sway, screenshot, empty_output):
        sway.capture_until_changed(screenshot, empty_output, client)
        sway.ipc("[app_id=\".*\"]", "kill")
        client.wait()
        return screenshot


class HelperLaunch:
    """A tool that first prints a ready word and its process id, and then prints ok after each command read from
    standard input."""

    piped = True

    def __init__(self, declaration):
        self.arguments = declaration.get("arguments", [])
        self.ready = declaration["ready"]

    def start(self, client, _sway):
        words = client.stdout.readline().split()
        assert len(words) == 2 and words[0] == self.ready, "the helper did not become ready"
        return psutil.Process(int(words[1]))

    def finish(self, client, sway, _screenshot, _empty_output):
        pointer_x, pointer_y = center_of(sway.output()["rect"])
        commands = [f"move {pointer_x} {pointer_y}", f"key {SPACE_BAR}"]
        client.stdin.write("".join(command + "\n" for command in commands))
        client.stdin.close()
        response = client.stdout.read()
        client.wait()
        assert response.splitlines() == ["ok"] * len(commands), response
        return None


LAUNCHES = {"window": WindowLaunch, "helper": HelperLaunch}


def packaged_apps(nix, flake, only):
    apps = json.loads(subprocess.check_output(
        nix + ["eval", "--offline", "--json", f"{flake}#apps.x86_64-linux"], text=True,
    ))
    names = sorted(name for name, app in apps.items()
                   if app.get("meta", {}).get("lavashellHost", "wayland") == "wayland")
    if only:
        assert set(only).issubset(names), "--only requires a packaged Wayland app"
        names = [name for name in names if name in only]
    return apps, names


def wrapped_executable(app, name):
    wrapper = Path(app["program"])
    assert wrapper.is_file(), f"build the {name} package before native verification"
    return wrapper.with_name(f".{wrapper.name}-wrapped").resolve(strict=True)


def run_app(nix, flake, name, app, sway, home, output, args, empty_output):
    executable = wrapped_executable(app, name)
    declaration = app["meta"]["lavashellLaunch"]
    launch = LAUNCHES[declaration["kind"]](declaration)
    app_home = home / name
    app_home.mkdir()
    client_env = {
        "PATH": "", "HOME": str(app_home), "LANG": "C.UTF-8",
        "XDG_RUNTIME_DIR": str(sway.runtime), "WAYLAND_DISPLAY": sway.display,
    }
    command = nix + ["run", "--offline", f"{flake}#{name}", "--"] + launch.arguments
    started = time.monotonic()
    with (output / f"{args.label}-{name}.log").open("w") as client_log:
        with scoped_process(
            command, cwd=app_home, env=client_env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE if launch.piped else client_log, stderr=client_log, text=True,
        ) as client:
            process = launch.start(client, sway)
            assert Path(process.exe()) == executable, f"{name} launched an unexpected executable"
            assert process.environ().get("PATH") == "", f"{name} changed its empty runtime PATH"
            screenshot = launch.finish(client, sway, output / f"{args.label}-{name}.png", empty_output)
            assert client.returncode == 0, f"{name} exited {client.returncode}"
    return {
        "name": name, "exit_code": client.returncode,
        "seconds": round(time.monotonic() - started, 3),
        "screenshot": screenshot.name if screenshot else None,
        "executable": str(executable),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--flake")
    parser.add_argument("--label", default="nix-run-native")
    parser.add_argument("--only", action="append", help="limit this run to named apps")
    parser.add_argument("--drm-device", help="render node of the compositor (default: chosen by the compositor)")
    parser.add_argument("--output-dir", type=Path, help="directory of the logs, screenshots and report "
                        "(default: a new temporary directory)")
    args = parser.parse_args()
    assert args.label and all(c.isalnum() or c in "-_" for c in args.label)
    flake = args.flake or f"git+file://{Path(__file__).resolve().parents[1]}"
    assert shutil.which("nix"), "run inside nix develop .#native"
    output = args.output_dir or Path(tempfile.mkdtemp(prefix="nix-run-native-"))
    output.mkdir(parents=True, exist_ok=True)
    nix = [shutil.which("nix"), "--extra-experimental-features", "nix-command flakes"]
    apps, names = packaged_apps(nix, flake, args.only)
    report = {"flake": flake, "all_wayland_apps": not args.only, "examples": []}
    compositor_log = output / f"{args.label}-compositor.log"
    with tempfile.TemporaryDirectory(prefix="nix-run-") as home, compositor_log.open("w") as log:
        with headless_sway(drm_device=args.drm_device, log=log) as sway:
            empty_output = sway.capture(output / f"{args.label}-empty.png")
            for name in names:
                sway.wait_for_windows(lambda windows: not windows)
                report["examples"].append(
                    run_app(nix, flake, name, apps[name], sway, Path(home), output, args, empty_output),
                )
                print(f"Passed nix run #{name} with empty PATH", flush=True)
    report_path = output / f"{args.label}.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(names)} nix run apps; report {report_path}", flush=True)


if __name__ == "__main__":
    with termination_signals():
        main()
