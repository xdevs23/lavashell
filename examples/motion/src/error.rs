// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::animation::{AnimationError, ContourError, KeyframesError};
use lavashell::prelude::NumberError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error("cannot sample a contour of the morph")]
    Contour(#[source] ContourError),
    #[error("cannot count the samples of the morph")]
    MorphSamples(#[source] NumberError),
    #[error("the keys of the timeline are invalid")]
    Timeline(#[source] KeyframesError),
    #[error("a motion of the demo cannot take its value")]
    Motion(#[source] AnimationError),
}
