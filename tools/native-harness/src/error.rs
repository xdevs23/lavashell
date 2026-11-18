// SPDX-License-Identifier: GPL-3.0-only
use crate::child_lines::ChildLinesError;
use crate::sway::SwayWindow;
use lavashell_process_scope::{ProcessError, ProcessExit};
use lavashell_wayland_input::protocol::Reply;
use rustix::io::Errno;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("{0} is not available")]
    MissingTool(&'static str),
    #[error("the declaration of {app} is invalid")]
    InvalidDeclaration {
        app: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{app} is a Wayland app without a launch")]
    MissingLaunch { app: String },
    #[error("the repository of this tool cannot be found")]
    FindRepository(#[source] std::io::Error),
    #[error("the repository path {0:?} cannot be written as a flake reference")]
    RepositoryUrl(PathBuf),
    #[error("the system name returned by Nix is not text")]
    UnreadableSystem(#[source] std::string::FromUtf8Error),
    #[error("the apps of the flake cannot be read")]
    UnreadableApps(#[source] serde_json::Error),
    #[error("--only requires packaged Wayland apps, but these are not: {}", .0.join(", "))]
    UnknownApps(Vec<String>),
    #[error("a temporary directory cannot be created")]
    CreateTemporaryDirectory(#[source] std::io::Error),
    #[error("the output directory {path:?} cannot be created")]
    CreateOutputDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the file {path:?} of the run cannot be created")]
    CreateRunFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("a log file cannot be shared by the output and the errors of a process")]
    DuplicateLog(#[source] std::io::Error),
    #[error("the runtime directory of the compositor cannot be created")]
    CreateRuntime(#[source] std::io::Error),
    #[error("the configuration of the compositor cannot be written")]
    WriteConfig(#[source] std::io::Error),
    #[error("the runtime directory of the compositor cannot be watched")]
    WatchRuntime(#[source] Errno),
    #[error("the compositor created the sockets {earlier:?} and {later:?} of the same kind")]
    AmbiguousSockets { earlier: OsString, later: OsString },
    #[error("the private compositor ended")]
    CompositorExited,
    #[error("swaymsg cannot be started")]
    StartSwaymsg(#[source] std::io::Error),
    #[error("swaymsg {request:?} {status} without a reply: {stderr}")]
    SwayRequestFailed {
        request: Vec<String>,
        status: ExitStatus,
        stderr: String,
    },
    #[error("sway answered {request:?} with a reply that cannot be read")]
    UnreadableSwayReply {
        request: Vec<String>,
        #[source]
        source: serde_json::Error,
    },
    #[error("sway refused the command {command:?}: {}", errors.join("; "))]
    SwayCommandFailed {
        command: Vec<String>,
        errors: Vec<String>,
    },
    #[error("sway has {0} outputs in place of one")]
    UnexpectedOutputs(usize),
    #[error("the subscription to the window events cannot be encoded")]
    EncodeSubscription(#[source] serde_json::Error),
    #[error("the window events were started without a pipe for their output")]
    NoEventPipe,
    #[error("the window events cannot be read")]
    ReadWindowEvents(#[source] std::io::Error),
    #[error("sway ended its window events")]
    WindowEventsEnded,
    #[error("the path of this tool cannot be found for the output capture")]
    LocateExecutable(#[source] std::io::Error),
    #[error("the output capture was started without pipes for its requests and replies")]
    NoCapturePipes,
    #[error("the capture request cannot be encoded")]
    EncodeCaptureRequest(#[source] serde_json::Error),
    #[error("the capture request cannot be sent")]
    SendCaptureRequest(#[source] std::io::Error),
    #[error("the reply of the output capture cannot be read")]
    ReadCaptureReply(#[source] ChildLinesError),
    #[error("the output capture {0} before it replied")]
    CaptureEnded(ProcessExit),
    #[error("the output capture wrote a line that is not a reply: {line:?}")]
    UnreadableCaptureReply {
        line: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the output capture failed: {message}")]
    CaptureFailed { message: String },
    #[error("the screenshot {path:?} cannot be read")]
    ReadImage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the package of {app} is not built")]
    PackageNotBuilt { app: String },
    #[error("the wrapped executable of {app} cannot be found")]
    FindWrappedExecutable {
        app: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the home directory of {app} cannot be created")]
    CreateAppHome {
        app: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the app ended before its window was ready")]
    AppEndedBeforeItsWindow,
    #[error("the app opened {count} windows in place of one: {0:?}", count = .0.len())]
    UnexpectedWindows(Vec<SwayWindow>),
    #[error("the app ended before its output changed")]
    AppEndedBeforeItsOutputChanged,
    #[error("the executable launched by {app} cannot be read")]
    ReadExecutable {
        app: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{app} launched the unexpected executable {executable:?}")]
    UnexpectedExecutable { app: String, executable: PathBuf },
    #[error("the environment of the process launched by {app} cannot be read")]
    ReadEnvironment {
        app: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{app} changed its empty runtime PATH to {path:?}")]
    PathNotEmpty { app: String, path: Option<OsString> },
    #[error("{app} {exit}")]
    AppFailed { app: String, exit: ProcessExit },
    #[error("the helper was started without pipes for its requests and replies")]
    NoHelperPipes,
    #[error("the reply of the helper cannot be read")]
    ReadHelperReply(#[source] ChildLinesError),
    #[error("the helper wrote a line that is not a reply: {line:?}")]
    UnreadableHelperReply {
        line: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the helper did not become ready, it replied {0:?}")]
    HelperNotReady(Reply),
    #[error("the helper ended before it became ready")]
    HelperEndedBeforeReady,
    #[error("the helper request cannot be encoded")]
    EncodeHelperRequest(#[source] serde_json::Error),
    #[error("the helper request cannot be sent")]
    SendHelperRequest(#[source] std::io::Error),
    #[error("the helper ended before it read every request")]
    HelperEndedBeforeCommands,
    #[error("the helper did not finish every request, its replies were {0:?}")]
    HelperFailed(Vec<Reply>),
    #[error("the report {path:?} cannot be written")]
    WriteReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the report cannot be encoded")]
    EncodeReport(#[source] serde_json::Error),
}
