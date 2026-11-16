// SPDX-License-Identifier: GPL-3.0-only
use lavashell_process_scope::{CommandLine, ProcessError, ProcessExit};
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum FileOperation {
    CheckExistence,
    CreateDirectory,
    CreateTemporary,
    Write,
    Lock,
    SetPermissions,
    Rename,
    KeepLog,
    List,
}

impl FileOperation {
    fn failure_message(self, path: &Path) -> String {
        let path = path.display();
        match self {
            Self::CheckExistence => format!("the tool cannot check whether {path} exists"),
            Self::CreateDirectory => format!("the tool cannot create the directory {path}"),
            Self::CreateTemporary => format!("the tool cannot create a temporary file in {path}"),
            Self::Write => format!("the tool cannot write {path}"),
            Self::Lock => format!("the tool cannot lock {path}"),
            Self::SetPermissions => format!("the tool cannot make {path} private"),
            Self::Rename => format!("the tool cannot move a new file to {path}"),
            Self::KeepLog => format!("the tool cannot save {path} as the log of the run"),
            Self::List => format!("the tool cannot list the directory {path}"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CommandStep {
    CheckAlignment,
    ReadSigningKey,
    CreateSigningKey,
    SignPackage,
    VerifySignature,
    StartAdbServer,
    StartEmulator,
    WaitForBoot,
    InstallPackage,
    RunInstrumentation,
    ReadDeviceLog,
}

impl fmt::Display for CommandStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CheckAlignment => "the tool cannot check the alignment of the package",
            Self::ReadSigningKey => "the tool cannot read the development signing key",
            Self::CreateSigningKey => "the tool cannot create the development signing key",
            Self::SignPackage => "the tool cannot sign the package",
            Self::VerifySignature => "the tool cannot verify the signature of the package",
            Self::StartAdbServer => "the tool cannot start the adb server",
            Self::StartEmulator => "the tool cannot start the emulator",
            Self::WaitForBoot => "the tool cannot wait until the device has booted",
            Self::InstallPackage => "the tool cannot install the package on the device",
            Self::RunInstrumentation => "the tool cannot run the round trips on the device",
            Self::ReadDeviceLog => "the tool cannot read the log of the device",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SigningKeyProblem {
    #[error("the keystore exists, but the password file next to it is missing")]
    PasswordMissing,
    #[error("keytool cannot export the certificate from the keystore")]
    CertificateUnreadable(#[source] ProcessError),
    #[error("the tool cannot decode the certificate exported by keytool")]
    CertificateMalformed(#[source] x509_cert::der::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum RoundTripError {
    #[error("{step}")]
    Command {
        step: CommandStep,
        #[source]
        source: ProcessError,
    },
    #[error("{}", operation.failure_message(path))]
    File {
        operation: FileOperation,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the home directory is unknown, so the tool cannot store the development signing key")]
    HomeUnknown,
    #[error("the operating system cannot provide random bytes for the signing password")]
    ReadRandomBytes(#[source] rustix::io::Errno),
    #[error(
        "the operating system provided {count} random bytes, fewer than the signing password needs"
    )]
    RandomBytesShort { count: usize },
    #[error("the tool cannot use the development signing key in {}", directory.display())]
    SigningKeyBroken {
        directory: PathBuf,
        #[source]
        problem: SigningKeyProblem,
    },
    #[error("{}", system_images_message(directory, abi, platforms))]
    SystemImageNotUnique {
        directory: PathBuf,
        abi: String,
        platforms: Vec<OsString>,
    },
    #[error("the adb server cannot listen on the loopback interface")]
    Listen(#[source] io::Error),
    #[error("the tool cannot give the log of the run to a started process")]
    ShareLog(#[source] io::Error),
    #[error("the emulator ended before the device finished booting")]
    EmulatorExited,
    #[error("the adb server ended before the device finished booting")]
    AdbServerExited,
    #[error("the command {command} {exit} while the device was booting")]
    BootWaitFailed {
        command: CommandLine,
        exit: ProcessExit,
    },
    #[error("the instrumentation printed output that is not UTF-8")]
    InstrumentationEncoding(#[source] std::string::FromUtf8Error),
    #[error("the instrumentation ended without round trip results:\n{output}")]
    InstrumentationCrashed { output: String },
    #[error("the instrumentation ended without printing a round trip result")]
    RoundTripsMissing,
    #[error("the instrumentation printed a line that is not a round trip result: {line:?}")]
    ResultMalformed {
        line: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the tool cannot encode the results as JSON")]
    EncodeResults(#[source] serde_json::Error),
    #[error("the round trips failed in the steps {}", steps.join(", "))]
    RoundTripsFailed { steps: Vec<String> },
}

impl RoundTripError {
    pub fn command(step: CommandStep) -> impl FnOnce(ProcessError) -> Self {
        move |source| Self::Command { step, source }
    }

    pub fn file(
        operation: FileOperation,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(io::Error) -> Self {
        move |source| Self::File {
            operation,
            path: path.into(),
            source,
        }
    }

    #[must_use]
    pub fn next_step(&self) -> NextStep<'_> {
        match self {
            Self::SigningKeyBroken { directory, .. } => NextStep::ReplaceSigningKey { directory },
            Self::HomeUnknown => NextStep::SetStateHome,
            Self::ResultMalformed { .. } | Self::EncodeResults(_) => NextStep::ReportDefect,
            Self::Command { source, .. } => NextStep::after_process_error(source),
            Self::File { .. }
            | Self::ReadRandomBytes(_)
            | Self::RandomBytesShort { .. }
            | Self::SystemImageNotUnique { .. }
            | Self::Listen(_)
            | Self::ShareLog(_)
            | Self::EmulatorExited
            | Self::AdbServerExited
            | Self::BootWaitFailed { .. }
            | Self::InstrumentationEncoding(_)
            | Self::InstrumentationCrashed { .. }
            | Self::RoundTripsMissing
            | Self::RoundTripsFailed { .. } => NextStep::ReadRunLogs,
        }
    }
}

fn system_images_message(directory: &Path, abi: &str, platforms: &[OsString]) -> String {
    let directory = directory.display();
    if platforms.is_empty() {
        return format!("the tool cannot find a system image for {abi} in {directory}");
    }
    let platforms: Vec<_> = platforms
        .iter()
        .map(|platform| platform.to_string_lossy())
        .collect();
    format!(
        "the tool needs one system image for {abi} in {directory}, but finds one for each of the platforms {}",
        platforms.join(", ")
    )
}

pub enum NextStep<'error> {
    ReadRunLogs,
    ReplaceSigningKey { directory: &'error Path },
    SetStateHome,
    ReportDefect,
}

impl NextStep<'_> {
    #[must_use]
    pub fn after_process_error(error: &ProcessError) -> Self {
        match error {
            ProcessError::Spawn { .. }
            | ProcessError::Failed { .. }
            | ProcessError::RefusedWhileEnding => Self::ReadRunLogs,
            ProcessError::CollectOutput { .. }
            | ProcessError::BecomeSubreaper(_)
            | ProcessError::OpenExitEvent(_)
            | ProcessError::Wait(_)
            | ProcessError::WaitForReadiness(_)
            | ProcessError::BlockSignals(_)
            | ProcessError::WatchSignals(_)
            | ProcessError::CreateFinishEvent(_)
            | ProcessError::ReceiveSignal(_) => Self::ReportDefect,
        }
    }
}

impl fmt::Display for NextStep<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadRunLogs => formatter.write_str(
                "Read the logs of the run, fix the cause named there, then run the tool again.",
            ),
            Self::ReplaceSigningKey { directory } => write!(
                formatter,
                "Delete the directory {}, so that the tool creates a new development signing key, then run the tool again.",
                directory.display()
            ),
            Self::SetStateHome => formatter.write_str(
                "Set XDG_STATE_HOME to the absolute path of a directory in which the tool can store the development signing key, then run the tool again.",
            ),
            Self::ReportDefect => formatter.write_str(
                "The tool or a program it ran is defective. Report this output to the developers of the project.",
            ),
        }
    }
}
