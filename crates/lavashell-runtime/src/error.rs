// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::units::NumberError;

/// A failure of the runtime, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("every surface id is used")]
    NumberSurface,
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
}
