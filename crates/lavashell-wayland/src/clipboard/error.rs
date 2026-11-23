// SPDX-License-Identifier: GPL-3.0-only
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::DragSourceId;

#[derive(Debug, thiserror::Error)]
pub enum DragSourceError {
    #[error("the runtime started the drag {0:?} while a drag with the same id is running")]
    DuplicateDrag(DragSourceId),
}

impl NamesNextStep for DragSourceError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::DuplicateDrag(_) => NextStep::ReportTheFailure,
        }
    }
}
