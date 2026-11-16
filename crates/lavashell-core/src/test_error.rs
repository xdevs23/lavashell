// SPDX-License-Identifier: GPL-3.0-only

use crate::animation::{
    AnimationError, BindError, ContourError, EasingError, KeyframesError, MotionError,
    PlaybackRateError, SpringError,
};
use crate::retained::UnbalancedRestore;
use crate::scene::test_sink::TestSinkError;
use crate::scene::tree::ApplyError;
use crate::scroll::ScrollError;
use crate::shape::PointsError;
use crate::transfer::TransferError;
use crate::units::NumberError;
use crate::view::{ComponentError, ComponentFailure, Failures, FrameError};
use std::num::TryFromIntError;

pub(crate) type TestResult = Result<(), TestError>;

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
    #[error("draw commands could not make a packet")]
    Packet(#[from] UnbalancedRestore),
    #[error("a transfer could not be made")]
    Transfer(#[from] TransferError),
    #[error("a frame of a view could not be made")]
    Frame(#[from] FrameError),
    #[error("a frame of a view could not be sent to the test sink")]
    SinkFrame(#[from] FrameError<TestSinkError>),
    #[error("the test sink failed a call")]
    Sink(#[from] TestSinkError),
    #[error("a component of a view failed")]
    Failed(Vec<ComponentFailure>),
    #[error("a scroll could not take a distance")]
    Scroll(#[from] ScrollError),
    #[cfg(feature = "serde")]
    #[error("a value could not be written or read")]
    Encoding(#[from] postcard::Error),
    #[error("no node has the tag {0:?}")]
    NoTag(crate::ui::NodeTag),
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

pub(crate) trait Required<T> {
    fn required(self, what: &'static str) -> Result<T, TestError>;
}

impl<T> Required<T> for Option<T> {
    fn required(self, what: &'static str) -> Result<T, TestError> {
        self.ok_or(TestError::Absent(what))
    }
}

pub(crate) trait Exactly<T> {
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

pub(crate) trait WithoutFailures {
    fn without_failures(self) -> Result<(), TestError>;
}

impl<E> WithoutFailures for Result<Failures, FrameError<E>>
where
    TestError: From<FrameError<E>>,
{
    fn without_failures(self) -> Result<(), TestError> {
        self?.without_failures()
    }
}

impl WithoutFailures for Failures {
    fn without_failures(self) -> Result<(), TestError> {
        self.into_result().map_err(TestError::Failed)
    }
}
