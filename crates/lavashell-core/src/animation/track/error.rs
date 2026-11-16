// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, VectorSpace};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnimationError {
    #[error("an animation value must be finite")]
    Value,
    #[error("an animation velocity must be finite")]
    Velocity,
}

pub(crate) fn finite<T: Animatable>(value: &T) -> Result<(), AnimationError> {
    if value.vector().finite() {
        Ok(())
    } else {
        Err(AnimationError::Value)
    }
}
