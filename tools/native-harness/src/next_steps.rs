// SPDX-License-Identifier: GPL-3.0-only
use crate::error::HarnessError;
use lavashell_process_scope::ProcessError;
use std::fmt;

pub enum NextStep {
    EnterNativeShell,
    ReadRunLogs,
    FixAppDeclaration,
    BuildPackage,
    FixArguments,
    ReportDefect,
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EnterNativeShell => {
                "Run the tool inside `nix develop .#native`, which provides the compositor."
            }
            Self::ReadRunLogs => {
                "Read the logs of the run, fix the cause named there, then run the tool again."
            }
            Self::FixAppDeclaration => concat!(
                "Correct the declaration of the app in the Nix files of the repository, ",
                "then run the tool again.",
            ),
            Self::BuildPackage => "Build the package named above, then run the tool again.",
            Self::FixArguments => "Correct the arguments of the command, then run it again.",
            Self::ReportDefect => concat!(
                "The tool or a program it ran is defective. ",
                "Report this output to the developers of the project.",
            ),
        })
    }
}

pub fn next_step(error: &HarnessError) -> NextStep {
    match error {
        HarnessError::Process(ProcessError::Spawn { .. })
        | HarnessError::MissingTool(_)
        | HarnessError::StartSwaymsg(_) => NextStep::EnterNativeShell,
        HarnessError::InvalidDeclaration { .. } | HarnessError::MissingLaunch { .. } => {
            NextStep::FixAppDeclaration
        }
        HarnessError::PackageNotBuilt { .. } => NextStep::BuildPackage,
        HarnessError::UnknownApps(_)
        | HarnessError::CreateOutputDirectory { .. }
        | HarnessError::CreateRunFile { .. } => NextStep::FixArguments,
        HarnessError::UnreadableSpellings(_)
        | HarnessError::DivergedHosts(_)
        | HarnessError::DivergedLaunchKinds(_)
        | HarnessError::FindRepository(_)
        | HarnessError::RepositoryUrl(_)
        | HarnessError::UnreadableSystem(_)
        | HarnessError::UnreadableApps(_)
        | HarnessError::UnreadableSwayReply { .. }
        | HarnessError::EncodeSubscription(_)
        | HarnessError::NoEventPipe
        | HarnessError::LocateExecutable(_)
        | HarnessError::NoCapturePipes
        | HarnessError::EncodeCaptureRequest(_)
        | HarnessError::UnreadableCaptureReply { .. }
        | HarnessError::NoHelperPipes
        | HarnessError::UnreadableHelperReply { .. }
        | HarnessError::EncodeHelperRequest(_)
        | HarnessError::EncodeReport(_) => NextStep::ReportDefect,
        HarnessError::Process(_)
        | HarnessError::CreateTemporaryDirectory(_)
        | HarnessError::DuplicateLog(_)
        | HarnessError::CreateRuntime(_)
        | HarnessError::WriteConfig(_)
        | HarnessError::WatchRuntime(_)
        | HarnessError::AmbiguousSockets { .. }
        | HarnessError::CompositorExited
        | HarnessError::SwayRequestFailed { .. }
        | HarnessError::SwayCommandFailed { .. }
        | HarnessError::UnexpectedOutputs(_)
        | HarnessError::ReadWindowEvents(_)
        | HarnessError::WindowEventsEnded
        | HarnessError::SendCaptureRequest(_)
        | HarnessError::ReadCaptureReply(_)
        | HarnessError::CaptureEnded(_)
        | HarnessError::CaptureFailed { .. }
        | HarnessError::ReadImage { .. }
        | HarnessError::FindWrappedExecutable { .. }
        | HarnessError::CreateAppHome { .. }
        | HarnessError::AppEndedBeforeItsWindow
        | HarnessError::UnexpectedWindows(_)
        | HarnessError::AppEndedBeforeItsOutputChanged
        | HarnessError::ReadExecutable { .. }
        | HarnessError::UnexpectedExecutable { .. }
        | HarnessError::ReadEnvironment { .. }
        | HarnessError::PathNotEmpty { .. }
        | HarnessError::AppFailed { .. }
        | HarnessError::ReadHelperReply(_)
        | HarnessError::HelperNotReady(_)
        | HarnessError::HelperEndedBeforeReady
        | HarnessError::SendHelperRequest(_)
        | HarnessError::HelperEndedBeforeCommands
        | HarnessError::HelperFailed(_)
        | HarnessError::WriteReport { .. } => NextStep::ReadRunLogs,
    }
}
