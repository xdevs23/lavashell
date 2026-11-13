// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::view::{ComponentFailure, FrameError};
use lavashell::prelude::{NumberError, Role};
use lavashell::renderer::Error as RendererError;
use std::{num::ParseFloatError, path::PathBuf, time::TryFromFloatSecsError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the example fonts need {} set to the directory of their dependencies",
        crate::FONT_DIRECTORY
    )]
    FontDirectory,
    #[error("cannot load the example fonts")]
    Fonts(#[source] RendererError),
    #[error("cannot create the shaper that measures the text of a view")]
    Shaper(#[source] RendererError),
    #[error("cannot create the directory {}", path.display())]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write the capture {}", path.display())]
    WriteCapture {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot show the frame of the view")]
    Frame(#[source] FrameError),
    #[error("a component of the view failed in a frame")]
    ComponentsFailed(Vec<ComponentFailure>),
    #[error("the view has no control tagged {0}")]
    MissingControl(String),
    #[error("the view shows no {role} named {name}")]
    MissingNode { role: Role, name: String },
    #[error("the app is not set up to run")]
    Setup(#[from] lavashell::SetupError),
    #[cfg(target_os = "linux")]
    #[error("the app failed")]
    App(#[source] lavashell::RunError),
    #[error("the app failed on the test platform")]
    Record(#[source] lavashell::PlatformRunError<lavashell_test_platform::TestPlatform>),
    #[error("the test platform returned no recorded frames")]
    NotServed(#[source] lavashell_test_platform::Error),
    #[error("the recording came to rest with {0} steps left to play")]
    UnplayedSteps(usize),
    #[error("the window {app_id} did not show a frame")]
    NothingShown { app_id: String },
    #[error("cannot create the frame log")]
    CreateFrameLog(#[source] csv::Error),
    #[error("cannot write a frame report to the frame log")]
    WriteFrameReport(#[source] csv::Error),
    #[error("cannot flush the frame log")]
    FlushFrameLog(#[source] std::io::Error),
    #[error("the value is not a number")]
    Number(#[source] ParseFloatError),
    #[error("a text scale must be finite and above zero")]
    TextScale,
    #[error("the value is not a duration in seconds")]
    Seconds(#[source] TryFromFloatSecsError),
    #[error("a length must be finite and at least zero")]
    Length(#[source] NumberError),
    #[error("a frame rate must be finite, above zero and at most one frame per nanosecond")]
    FrameRate,
    #[error("cannot decode an icon of the example")]
    Icon(#[source] lavashell_lucide::Error),
}

/// Fails with [`Error::ComponentsFailed`] when `failures` contains a failed component.
pub(crate) fn reject_failures(
    failures: impl IntoIterator<Item = ComponentFailure>,
) -> Result<(), Error> {
    let failures: Vec<_> = failures.into_iter().collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::ComponentsFailed(failures))
    }
}
