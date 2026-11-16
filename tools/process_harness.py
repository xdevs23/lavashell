# SPDX-License-Identifier: GPL-3.0-only

from pathlib import Path
import signal
import subprocess
import sys

from process_scope import piped, scoped_process, termination_signals

WRAPPER = Path(__file__).with_name("process_wrapper.py")

with termination_signals():
    with scoped_process(
        [sys.executable, str(WRAPPER), "--ignore-term"], stdout=subprocess.PIPE, text=True,
    ) as wrapper:
        child = piped(wrapper.stdout).readline().strip()
        print(wrapper.pid, child, flush=True)
        signal.pause()
