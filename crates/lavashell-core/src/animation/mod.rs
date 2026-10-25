// SPDX-License-Identifier: GPL-3.0-only
//! Motion computed from time in closed form, animations driven by reactive state, and interpolation between the
//! contours of shapes.

#[cfg(test)]
mod color_tests;
mod easing;
mod keyframes;
mod morph;
mod motion_scale;
mod motion_spec;
mod parts;
#[cfg(test)]
mod playing_tests;
mod program;
#[cfg(test)]
mod reduced_motion_tests;
#[cfg(test)]
mod shape_tests;
mod spring;
#[cfg(test)]
mod spring_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timer_tests;
mod track;
mod value;

pub use easing::{Easing, EasingError};
pub use keyframes::{Direction, Keyframe, Keyframes, KeyframesError, Playback, Repeat};
pub use morph::{Contour, ContourError};
pub use motion_spec::{MotionSpec, Tween};
pub(crate) use parts::{Fading, entry};
pub use program::{MotionError, Playing, Program, Timing};
pub use spring::{Spring, SpringError};
pub(crate) use track::finite;
pub use track::{Animation, AnimationError, BindError, Bindable, Motion, PlaybackRateError, Timer};
pub use value::{Animatable, Composite, Trajectory, Vector, VectorSpace};
pub(crate) use value::{composite_fields, nested};
