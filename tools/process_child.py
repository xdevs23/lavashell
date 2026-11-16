# SPDX-License-Identifier: GPL-3.0-only

import argparse
import signal

parser = argparse.ArgumentParser()
parser.add_argument("--ignore-term", action="store_true")
arguments = parser.parse_args()
if arguments.ignore_term:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
print("ready", flush=True)
signal.pause()
