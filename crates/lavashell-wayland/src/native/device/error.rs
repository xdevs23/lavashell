// SPDX-License-Identifier: GPL-3.0-only
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("the serial of an input event does not fit a Wayland serial")]
    ProtocolSerial(#[source] TryFromIntError),
}

impl NamesNextStep for DeviceError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::ProtocolSerial(_) => NextStep::ReportTheFailure,
        }
    }
}
