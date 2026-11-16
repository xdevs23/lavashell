// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use std::fmt;

pub enum NextStep {
    StartCompositor,
    UseCompositorWithVirtualInput,
    ConfigureCompositor,
    RestartConnection,
    FixRequest,
    KeepPipesOpen,
    ProvideRuntimeDirectory,
    RebuildInDevelopmentShell,
    EnableKeyboard,
    ReportDefect,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StartCompositor => concat!(
                "Start the compositor, or set `WAYLAND_DISPLAY` to its socket, ",
                "then run the helper again.",
            ),
            Self::UseCompositorWithVirtualInput => concat!(
                "Use a compositor that offers the virtual pointer and virtual keyboard protocols, ",
                "then run the helper again.",
            ),
            Self::ConfigureCompositor => concat!(
                "Run the compositor with one seat and at least one output that has a position ",
                "and a size, then run the helper again.",
            ),
            Self::RestartConnection => concat!(
                "Check that the compositor is still running, ",
                "then run the helper again.",
            ),
            Self::FixRequest => concat!(
                "Send each request as one line of JSON: the command `move` with the numbers `x` ",
                "and `y`, or the command `key` with the `name` of a key in the keymap.",
            ),
            Self::KeepPipesOpen => concat!(
                "Keep the standard input and output of the helper open ",
                "until the last reply has been read.",
            ),
            Self::ProvideRuntimeDirectory => concat!(
                "Set `XDG_RUNTIME_DIR` to a directory with free space ",
                "that the current user can write to.",
            ),
            Self::RebuildInDevelopmentShell => "Build the helper again inside `nix develop`.",
            Self::EnableKeyboard => "Start the helper with `--keyboard`.",
            Self::ReportDefect => {
                "The helper is defective. Report this output to the developers of the project."
            }
        })
    }
}

pub fn next_step(error: &SessionError) -> NextStep {
    match error {
        SessionError::Connect(_) => NextStep::StartCompositor,
        SessionError::Bind(_) => NextStep::UseCompositorWithVirtualInput,
        SessionError::NoSeat
        | SessionError::AmbiguousSeat
        | SessionError::NoOutput
        | SessionError::OutputWithoutLogicalRectangle
        | SessionError::ExtentOutOfRange(_) => NextStep::ConfigureCompositor,
        SessionError::Globals(_) | SessionError::Dispatch(_) | SessionError::Flush(_) => {
            NextStep::RestartConnection
        }
        SessionError::ParseRequest(_) | SessionError::UnknownKey(_) => NextStep::FixRequest,
        SessionError::ReadRequest(_) | SessionError::SendReply(_) => NextStep::KeepPipesOpen,
        SessionError::RuntimeDirectoryMissing
        | SessionError::CreateKeymapFile(_)
        | SessionError::WriteKeymap(_) => NextStep::ProvideRuntimeDirectory,
        SessionError::KeymapData(_) | SessionError::Keymap(_) => {
            NextStep::RebuildInDevelopmentShell
        }
        SessionError::KeyboardDisabled => NextStep::EnableKeyboard,
        SessionError::EncodeReply(_) | SessionError::KeymapTooLarge(_) => NextStep::ReportDefect,
    }
}
