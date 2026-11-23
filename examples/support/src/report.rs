// SPDX-License-Identifier: GPL-3.0-only
use crate::ExampleError;
use lavashell::error_chain::message_chain;
use std::{fmt, process::ExitCode};

pub fn report(operation: &str, error: impl ExampleError) {
    eprint!("{}", FailedOperation { operation, error });
}

pub fn report_failure(operation: &str, outcome: Result<(), impl ExampleError>) {
    if let Err(error) = outcome {
        report(operation, error);
    }
}

pub fn exit_code(run: Result<(), impl ExampleError>) -> ExitCode {
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report("the example stopped", error);
            ExitCode::FAILURE
        }
    }
}

struct FailedOperation<'a, E> {
    operation: &'a str,
    error: E,
}

impl<E: ExampleError> fmt::Display for FailedOperation<'_, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{}: {}",
            self.operation,
            message_chain(&self.error)
        )?;
        writeln!(formatter, "{}", self.error.next_step())
    }
}

#[cfg(test)]
mod tests;
