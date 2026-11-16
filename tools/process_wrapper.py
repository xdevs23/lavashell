# SPDX-License-Identifier: GPL-3.0-only

import argparse
from pathlib import Path
import signal
import subprocess
import sys

from process_scope import piped

parser = argparse.ArgumentParser()
parser.add_argument("--ignore-term", action="store_true")
parser.add_argument("--exit", action="store_true")
arguments = parser.parse_args()
command = [sys.executable, str(Path(__file__).with_name("process_child.py"))]
if arguments.ignore_term:
    command.append("--ignore-term")
child = subprocess.Popen(command, stdout=subprocess.PIPE, text=True)
if piped(child.stdout).readline() != "ready\n":
    sys.exit("the child did not become ready")
print(child.pid, flush=True)
if not arguments.exit:
    signal.pause()
