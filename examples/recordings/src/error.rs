// SPDX-License-Identifier: GPL-3.0-only
use crate::RecordingPlatform;
use lavashell::PlatformRunError;
use lavashell::error_chain::next_step::NextStep;
use lavashell_example_support::{ExampleError, ExampleStep};
use std::path::PathBuf;
use std::time::TryFromFloatSecsError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Keyboard(#[from] lavashell_on_screen_keyboard::Error),
    #[error(transparent)]
    Taskbar(#[from] lavashell_taskbar::Error),
    #[error(transparent)]
    Materials(#[from] lavashell_materials_example::Error),
    #[error(transparent)]
    MusicPlayer(#[from] lavashell_music_player::Error),
    #[error("the keyboard scene does not show a search field")]
    MissingField,
    #[error("the app failed on the test platform")]
    Record(#[source] PlatformRunError<RecordingPlatform>),
    #[error("the test platform did not return the recorded frames")]
    NotServed(#[source] lavashell_test_platform::Error),
    #[error("the recording ended with {0} steps left to play")]
    UnplayedSteps(usize),
    #[error("the test platform cannot press the keys of a step")]
    Press(#[source] lavashell_test_platform::Error),
    #[error("the view does not give a delay for the next step")]
    UnreadDelay,
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
    #[error("a frame rate of {rate} frames per second is not above zero")]
    FrameRateNotPositive {
        rate: f64,
        #[source]
        source: TryFromFloatSecsError,
    },
    #[error("a frame rate of {rate} frames per second is above one billion")]
    FrameRateAboveOnePerNanosecond { rate: f64 },
    #[error("the window {app_id} did not show a frame")]
    NothingShown { app_id: String },
}

impl ExampleError for Error {
    fn next_step(&self) -> ExampleStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Keyboard(error) => error.next_step(),
            Self::Taskbar(error) => error.next_step(),
            Self::Materials(error) => error.next_step(),
            Self::MusicPlayer(error) => error.next_step(),
            Self::Record(error) => error.next_step(),
            Self::FrameRateNotPositive { .. } | Self::FrameRateAboveOnePerNanosecond { .. } => {
                ExampleStep::CorrectOptions
            }
            Self::CreateDirectory { .. } | Self::WriteCapture { .. } => {
                ExampleStep::ChooseWritableCapture
            }
            Self::NotServed(_)
            | Self::UnplayedSteps(_)
            | Self::Press(_)
            | Self::UnreadDelay
            | Self::MissingField
            | Self::NothingShown { .. } => NextStep::ReportTheFailure.into(),
        }
    }
}
