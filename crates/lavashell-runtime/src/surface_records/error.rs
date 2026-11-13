// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::units::NumberError;

/// The failures that can occur in the surface records.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("every surface id is used")]
    NumberSurface,
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
}
