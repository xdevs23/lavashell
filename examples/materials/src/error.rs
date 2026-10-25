// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::animation::{AnimationError, KeyframesError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error("the keys of the phase loop are invalid")]
    PhaseLoop(#[source] KeyframesError),
    #[error("the motion of the phase loop cannot be made")]
    PhaseMotion(#[source] AnimationError),
}
