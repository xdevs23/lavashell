// SPDX-License-Identifier: GPL-3.0-only
use crate::ExampleError;
use lavashell::core::error::message_chain;
use std::{error::Error, process::ExitCode};

pub fn report(operation: &str, error: impl Error) {
    eprintln!("{operation}: {}", message_chain(error));
}

pub fn report_failure(operation: &str, outcome: Result<(), impl Error>) {
    if let Err(error) = outcome {
        report(operation, error);
    }
}

pub fn exit_code(run: Result<(), impl ExampleError>) -> ExitCode {
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let next_step = error.next_step();
            report("the example stopped", error);
            eprintln!("{next_step}");
            ExitCode::FAILURE
        }
    }
}
