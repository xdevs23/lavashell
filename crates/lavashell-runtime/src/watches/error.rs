// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::transfer::Cancellation;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    #[error("a new watch could not get an id")]
    NumberWatch(#[source] IdsExhausted),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::NumberWatch(_) => NextStep::ReportTheFailure,
            Self::Cancelled(cancellation) => cancellation.next_step(),
        }
    }
}
