// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, VectorSpace};

/// The error returned when a value or a velocity given to an animation is not finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnimationError {
    #[error("an animation value must be finite")]
    Value,
    #[error("an animation velocity must be finite")]
    Velocity,
}

/// Returns an error when a component of the vector of `value` is not finite.
pub(crate) fn finite<T: Animatable>(value: &T) -> Result<(), AnimationError> {
    if value.vector().finite() {
        Ok(())
    } else {
        Err(AnimationError::Value)
    }
}
