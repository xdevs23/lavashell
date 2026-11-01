// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::units::NumberError;
use std::num::TryFromIntError;

/// A failure of the runtime, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new surface has a number too large for a surface id")]
    NumberSurface(#[source] TryFromIntError),
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
}
