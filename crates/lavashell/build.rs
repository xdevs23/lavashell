// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::process::ExitCode;

use lavashell_build_script::{generate_registrations, run};

fn main() -> ExitCode {
    run(generate_registrations)
}
