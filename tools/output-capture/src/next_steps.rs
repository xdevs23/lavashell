// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use std::fmt;

pub enum NextStep {
    StartCompositor,
    UseCompositorWithScreencopy,
    ConfigureCompositor,
    RestartConnection,
    FixRequest,
    KeepPipesOpen,
    FreeMemory,
    FixPixelsPath,
    ReportDefect,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StartCompositor => concat!(
                "Start the compositor, or set `WAYLAND_DISPLAY` to its socket, ",
                "then run the capture again.",
            ),
            Self::UseCompositorWithScreencopy => concat!(
                "Use a compositor that offers version 3 of the wlr screencopy protocol ",
                "and shared memory buffers, then run the capture again.",
            ),
            Self::ConfigureCompositor => {
                "Run the compositor with exactly one output, then run the capture again."
            }
            Self::RestartConnection => concat!(
                "Check that the compositor is still running, ",
                "then run the capture again.",
            ),
            Self::FixRequest => concat!(
                "Send each request as one line of JSON: the command `capture_change` with the path ",
                "`pixels` of the file that receives the pixels of the frame.",
            ),
            Self::KeepPipesOpen => concat!(
                "Keep the standard input and output of the capture open ",
                "until the last reply has been read.",
            ),
            Self::FreeMemory => "Free memory for a copy of the output, then run the capture again.",
            Self::FixPixelsPath => {
                "Name a `pixels` path in a directory that exists and that the current user can write to."
            }
            Self::ReportDefect => {
                "The capture is defective. Report this output to the developers of the project."
            }
        })
    }
}

pub fn next_step(error: &SessionError) -> NextStep {
    match error {
        SessionError::Connect(_) => NextStep::StartCompositor,
        SessionError::Bind(_)
        | SessionError::NoSharedMemoryBuffer
        | SessionError::UnsupportedFormat(_) => NextStep::UseCompositorWithScreencopy,
        SessionError::NoOutput | SessionError::AmbiguousOutput => NextStep::ConfigureCompositor,
        SessionError::Globals(_)
        | SessionError::Dispatch(_)
        | SessionError::Flush(_)
        | SessionError::CopyFailed => NextStep::RestartConnection,
        SessionError::ParseRequest(_) => NextStep::FixRequest,
        SessionError::ReadRequest(_) | SessionError::SendReply(_) => NextStep::KeepPipesOpen,
        SessionError::CreatePool(_) | SessionError::ResizePool(_) => NextStep::FreeMemory,
        SessionError::WritePixels(_) => NextStep::FixPixelsPath,
        SessionError::EncodeReply(_)
        | SessionError::FrameOutOfRange(_)
        | SessionError::FrameTooLarge
        | SessionError::ReadyBeforeCopy => NextStep::ReportDefect,
    }
}
