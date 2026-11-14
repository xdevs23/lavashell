// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::error::message_chain;
use std::{error::Error, process::ExitCode};

/// Writes `error` and each of its causes to standard error, after `operation`, which describes what the example
/// was doing when the error occurred.
pub fn report(operation: &str, error: impl Error) {
    eprintln!("{operation}: {}", message_chain(error));
}

/// Writes the error in `outcome`, if there is one, the way `report` does. A handler uses this function when it cannot
/// return the error to a caller.
pub fn report_failure(operation: &str, outcome: Result<(), impl Error>) {
    if let Err(error) = outcome {
        report(operation, error);
    }
}

/// The exit code for the result of a run of an example. A failed run is first reported to standard error.
pub fn exit_code(run: Result<(), impl Error>) -> ExitCode {
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report("the example stopped", error);
            ExitCode::FAILURE
        }
    }
}
