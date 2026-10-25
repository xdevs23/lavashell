// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::AnimationError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error("the scene has no search field")]
    MissingField,
    #[error("the keyboard of the scene cannot be made")]
    Keyboard(#[source] AnimationError),
}
