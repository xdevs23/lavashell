// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::units::NumberError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new surface could not get an id")]
    NumberSurface(#[source] IdsExhausted),
    #[error("a node of the accessibility tree of a surface could not get an id")]
    NumberAccessibleNode(#[source] IdsExhausted),
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
}
