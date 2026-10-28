// SPDX-License-Identifier: GPL-3.0-only
//! The errors with which a test of the crate fails. Each error converts into `TestError`, so a test can return it
//! with `?`.

use crate::animation::{
    AnimationError, BindError, ContourError, EasingError, KeyframesError, MotionError,
    PlaybackRateError, SpringError,
};
use crate::scene::tree::{ApplyError, SceneUpdate};
use crate::scroll::ScrollError;
use crate::shape::PointsError;
use crate::transfer::TransferError;
use crate::units::NumberError;
use crate::view::{ComponentError, ComponentFailure, Failures, Frame, FrameError};
use std::num::TryFromIntError;

pub(crate) type TestResult = Result<(), TestError>;

/// The reason a test stops: an error from the code under test, or a value that the test needs and did not get.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("a spring could not be made")]
    Spring(#[from] SpringError),
    #[error("an easing curve could not be made")]
    Easing(#[from] EasingError),
    #[error("keyframes could not be made")]
    Keyframes(#[from] KeyframesError),
    #[error("a motion could not be made")]
    Motion(#[from] MotionError),
    #[error("a contour could not be sampled")]
    Contour(#[from] ContourError),
    #[error("an animation could not take a value")]
    Animation(#[from] AnimationError),
    #[error("a playback rate could not be set")]
    PlaybackRate(#[from] PlaybackRateError),
    #[error("an animation could not be bound to a motion clock")]
    Bind(#[from] BindError),
    #[error("a shape could not be made from its points")]
    Points(#[from] PointsError),
    #[error("a number could not be converted")]
    Number(#[from] NumberError),
    #[error("an integer does not fit the type it is converted to")]
    Integer(#[from] TryFromIntError),
    #[error("a scene update could not be applied")]
    Update(#[from] ApplyError),
    #[error("a transfer could not be made")]
    Transfer(#[from] TransferError),
    #[error("a frame of a view could not be made")]
    Frame(#[from] FrameError),
    #[error("a component of a view failed")]
    Failed(Vec<ComponentFailure>),
    #[error("a scroll could not take a distance")]
    Scroll(#[from] ScrollError),
    #[cfg(feature = "serde")]
    #[error("a value could not be written or read")]
    Encoding(#[from] postcard::Error),
    #[error("no node has the tag {0:?}")]
    NoTag(String),
    #[error("no text paints {0:?}")]
    NoText(String),
    #[error("the test needs {0}, which it does not get")]
    Absent(&'static str),
    #[error("the number of {what} is {found}, and the test needs {expected}")]
    Count {
        what: &'static str,
        expected: usize,
        found: usize,
    },
    #[error("no component failed in a frame where the test needs a failure")]
    NothingFailed,
    #[error("a component failed with another error than the test needs")]
    Component(#[from] ComponentError),
}

/// Turns an optional value that a test needs into the value or a test failure.
pub(crate) trait Required<T> {
    /// The value, or `TestError::Absent` with `what` as the description of the missing value.
    fn required(self, what: &'static str) -> Result<T, TestError>;
}

impl<T> Required<T> for Option<T> {
    fn required(self, what: &'static str) -> Result<T, TestError> {
        self.ok_or(TestError::Absent(what))
    }
}

/// Turns a list into an array of exactly the number of items that a test needs, or into a test failure.
pub(crate) trait Exactly<T> {
    /// The `N` items as an array, or `TestError::Count` with `what` as the description of the items when the list
    /// has a different length.
    fn exactly<const N: usize>(self, what: &'static str) -> Result<[T; N], TestError>;
}

impl<T> Exactly<T> for Vec<T> {
    fn exactly<const N: usize>(self, what: &'static str) -> Result<[T; N], TestError> {
        <[T; N]>::try_from(self).map_err(|items| TestError::Count {
            what,
            expected: N,
            found: items.len(),
        })
    }
}

/// Turns the result of a frame into what the frame shows, or into a test failure where the frame failed or a
/// component failed in it.
pub(crate) trait WithoutFailures {
    type Shown;

    fn without_failures(self) -> Result<Self::Shown, TestError>;
}

impl WithoutFailures for Result<Frame, FrameError> {
    type Shown = SceneUpdate;

    fn without_failures(self) -> Result<SceneUpdate, TestError> {
        let Frame { update, failures } = self?;
        failures.without_failures()?;
        Ok(update)
    }
}

impl WithoutFailures for Result<Failures, FrameError> {
    type Shown = ();

    fn without_failures(self) -> Result<(), TestError> {
        self?.without_failures()
    }
}

impl WithoutFailures for Failures {
    type Shown = ();

    fn without_failures(self) -> Result<(), TestError> {
        let failures: Vec<_> = self.into_iter().collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(TestError::Failed(failures))
        }
    }
}
