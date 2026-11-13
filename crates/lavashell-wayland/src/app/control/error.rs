// SPDX-License-Identifier: GPL-3.0-only
use crate::LayerLayoutError;
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::units::NumberError;
use sctk::reexports::client::backend::WaylandError;
use smithay_client_toolkit as sctk;
use std::num::TryFromIntError;

/// A failure of a request for the size or the state of a native surface, sent with a frame or with a commit without a
/// frame.
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
