// SPDX-License-Identifier: GPL-3.0-only
use crate::LayerLayoutError;
use crate::error::next_step_after;
use crate::sctk;
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::units::NumberError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use sctk::reexports::client::backend::WaylandError;
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("no buffer of whole pixels covers one logical pixel")]
    BufferSize(#[source] NumberError),
    #[error("the buffer scale of a surface does not fit the protocol")]
    BufferScale(#[source] TryFromIntError),
    #[error("the size {0:?} of a surface does not fit the viewport destination")]
    Destination(Size),
    #[error("the rectangle {0:?} of a region does not fit the protocol")]
    Region(Rect),
    #[error("the layout of a layer does not fit the protocol")]
    Layout(#[from] LayerLayoutError),
    #[error("the requests of a surface could not be sent to the compositor")]
    Flush(#[source] WaylandError),
}

impl NamesNextStep for ControlError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Flush(error) => next_step_after(error),
            Self::BufferSize(_)
            | Self::BufferScale(_)
            | Self::Destination(_)
            | Self::Region(_)
            | Self::Layout(_) => NextStep::ReportTheFailure,
        }
    }
}
