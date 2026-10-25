# SPDX-License-Identifier: GPL-3.0-only
"""A private headless sway session with its own runtime directory."""

from contextlib import ExitStack, contextmanager
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile

from inotify_simple import INotify, flags
from PIL import Image, ImageChops

from process_scope import exit_event, scoped_process


class HeadlessSway:
    def __init__(self, runtime, environment, tools):
        self.runtime = runtime
        self.environment = environment
        self.display = environment["WAYLAND_DISPLAY"]
        self.tools = tools

    def ipc(self, *command):
        result = json.loads(subprocess.check_output(
            [self.tools["swaymsg"], *command], env=self.environment, text=True,
        ))
        if isinstance(result, list) and result and "success" in result[0]:
            assert all(item["success"] for item in result), result
        return result

    def output(self):
        """The only output of the session."""
        [output] = self.ipc("-t", "get_outputs")
        return output

    def windows(self):
        def collect(node):
            found = [node] if node.get("app_id") else []
            for child in node.get("nodes", []) + node.get("floating_nodes", []):
                found.extend(collect(child))
            return found

        return collect(self.ipc("-t", "get_tree"))

    def wait_for_windows(self, ready, client=None):
        """Block on the window events of sway until `ready(windows)` returns true, and return the windows."""
        # Sway sends a tick event when the subscription starts. The first check therefore also sees the windows that
        # existed before the subscription.
        subscription = [self.tools["swaymsg"], "-t", "subscribe", "-m", '["tick", "window"]']
        with ExitStack() as scope:
            events = scope.enter_context(
                scoped_process(subscription, env=self.environment, stdout=subprocess.PIPE),
            )
            watched = [events.stdout]
            if client is not None:
                client_exited = scope.enter_context(exit_event(client))
                watched.append(client_exited)
            while True:
                readable, _, _ = select.select(watched, [], [])
                assert client is None or client_exited not in readable, "the app ended before its windows were ready"
                assert events.stdout.read1(), "sway ended its window events"
                windows = self.windows()
                if ready(windows):
                    return windows

    def capture(self, path):
        subprocess.run([self.tools["grim"], str(path)], env=self.environment, check=True)
        return path

    def capture_until_changed(self, path, reference, client):
        """Capture the output until it differs from the image at `reference`."""
        # Sway sends no event for a new frame, so the output is captured again and again. Each capture with grim waits
        # for the next frame.
        with Image.open(reference) as before:
            while True:
                assert client.poll() is None, "the app ended before its output changed"
                with Image.open(self.capture(path)) as after:
                    if ImageChops.difference(before.convert("RGB"), after.convert("RGB")).getbbox():
                        return path


def wait_for_sockets(runtime, compositor):
    """Block until sway has created its IPC socket. Sway creates the IPC socket after the Wayland socket, so both exist
    on return."""
    with INotify() as watch, exit_event(compositor) as compositor_exited:
        watch.add_watch(runtime, flags.CREATE)
        while not list(runtime.glob("sway-ipc.*.sock")):
            readable, _, _ = select.select([watch, compositor_exited], [], [])
            assert compositor_exited not in readable, "private compositor failed"
            watch.read()
    return next(runtime.glob("sway-ipc.*.sock"))


@contextmanager
def headless_sway(*, drm_device, log):
    """Start sway on one headless output of its default size, writing its output to the open file `log`."""
    tools = {name: shutil.which(name) for name in ("sway", "swaymsg", "grim")}
    assert all(tools.values()), "run inside nix develop .#native"
    with tempfile.TemporaryDirectory(prefix="headless-sway-") as directory:
        runtime = Path(directory)
        config = runtime / "sway.conf"
        config.write_text(
            "default_border none\ndefault_floating_border none\n"
            "xwayland disable\nseat seat0 fallback true\n"
        )
        environment = os.environ.copy()
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK"):
            environment.pop(key, None)
        environment.update(
            XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
            WLR_LIBINPUT_NO_DEVICES="1", WLR_RENDERER="gles2",
        )
        if drm_device:
            environment["WLR_RENDER_DRM_DEVICE"] = drm_device
        with scoped_process(
            [tools["sway"], "--unsupported-gpu", "--config", str(config)],
            env=environment, stdout=log, stderr=subprocess.STDOUT,
        ) as compositor:
            environment["SWAYSOCK"] = str(wait_for_sockets(runtime, compositor))
            environment["WAYLAND_DISPLAY"] = next(
                path.name for path in runtime.glob("wayland-*") if not path.name.endswith(".lock")
            )
            yield HeadlessSway(runtime, environment, tools)
