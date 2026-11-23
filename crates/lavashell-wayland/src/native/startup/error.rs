// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("the runtime requested a window whose minimum size {0:?} does not fit the protocol")]
    MinimumSizeBeyondProtocol(Size),
    #[error("the compositor does not support xdg-shell")]
    NoXdgShell,
}

impl NamesNextStep for WindowError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::MinimumSizeBeyondProtocol(_) => NextStep::ReportTheFailure,
            Self::NoXdgShell => NextStep::UseSupportingPlatform,
        }
    }
}
