// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum FocusGrabError {
    #[error(
        "the runtime made a focus grab request, but the compositor does not support focus grabs"
    )]
    NoFocusGrab,
    #[error(
        "the runtime made a focus grab request for the surface {0:?}, which has no native surface"
    )]
    WithoutSurface(SurfaceId),
}

impl NamesNextStep for FocusGrabError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::NoFocusGrab => NextStep::UseSupportingPlatform,
            Self::WithoutSurface(_) => NextStep::ReportTheFailure,
        }
    }
}
