// SPDX-License-Identifier: GPL-3.0-only
use super::FailedOperation;
use crate::{ExampleError, ExampleStep};
use lavashell::error_chain::next_step::NextStep;
use std::{error::Error, fmt};

#[derive(Debug)]
struct Cause;

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("cause")
    }
}

impl Error for Cause {}

#[derive(Debug)]
struct Stopped(Cause);

impl fmt::Display for Stopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("stopped")
    }
}

impl Error for Stopped {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl ExampleError for Stopped {
    fn next_step(&self) -> ExampleStep {
        NextStep::FreeSystemResources.into()
    }
}

#[test]
fn a_failed_operation_prints_the_cause_chain_then_the_next_step_on_a_line_of_its_own() {
    let expected = format!(
        "the example stopped: stopped: cause\n{}\n",
        NextStep::FreeSystemResources
    );
    let failed = FailedOperation {
        operation: "the example stopped",
        error: Stopped(Cause),
    };
    assert_eq!(failed.to_string(), expected);
}
