// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::scene::tree::SceneUpdate;
use lavashell_core::view::{ComponentError, ComponentFailure, Failures, Frame, FrameError};

/// A frame of a view that failed, or in which a component failed.
#[derive(Debug, thiserror::Error)]
pub enum FrameFailure {
    #[error("a frame of a view could not be made")]
    Frame(#[from] FrameError),
    #[error("a component of a view failed in a frame")]
    Components(Vec<ComponentFailure>),
}

/// Turns the result of a frame into what the frame shows, or into a failure where the frame failed or a component
/// failed in it.
pub trait WithoutFailures {
    type Shown;

    /// # Errors
    /// Fails where the frame failed or a component failed in it.
    fn without_failures(self) -> Result<Self::Shown, FrameFailure>;
}

impl WithoutFailures for Result<Frame, FrameError> {
    type Shown = SceneUpdate;

    fn without_failures(self) -> Result<SceneUpdate, FrameFailure> {
        let Frame { update, failures } = self?;
        none_failed(failures)?;
        Ok(update)
    }
}

impl WithoutFailures for Result<Failures, FrameError> {
    type Shown = ();

    fn without_failures(self) -> Result<(), FrameFailure> {
        none_failed(self?)
    }
}

fn none_failed(failures: Failures) -> Result<(), FrameFailure> {
    let failures: Vec<_> = failures.into_iter().collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(FrameFailure::Components(failures))
    }
}

/// The causes of the component failures with which `result` failed, in the order of the failures.
///
/// # Errors
/// Passes on the error of `result` where the frame itself failed.
pub fn failure_causes<T>(
    result: Result<T, FrameFailure>,
) -> Result<Vec<ComponentError>, FrameFailure> {
    match result {
        Ok(_) => Ok(Vec::new()),
        Err(FrameFailure::Components(failures)) => {
            Ok(failures.into_iter().map(|failure| failure.cause).collect())
        }
        Err(error) => Err(error),
    }
}
