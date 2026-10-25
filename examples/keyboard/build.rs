// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use std::process::ExitCode;

fn main() -> ExitCode {
    lavashell_example_copy_keys::build("copy/keyboard.en.tsv")
}
