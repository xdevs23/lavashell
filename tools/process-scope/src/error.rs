// SPDX-License-Identifier: GPL-3.0-only
use crate::checked_command::CommandLine;
use crate::process_exit::ProcessExit;
use rustix::io::Errno;

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("the command {command} cannot be started")]
    Spawn {
        command: CommandLine,
        #[source]
        source: std::io::Error,
    },
    #[error("this program cannot read the output from the command {command}")]
    CollectOutput {
        command: CommandLine,
        #[source]
        source: std::io::Error,
    },
    #[error("the command {command} {exit}:\n{stderr}")]
    Failed {
        command: CommandLine,
        exit: ProcessExit,
        stderr: String,
    },
    #[error("this program cannot start a process while it ends on a termination signal")]
    RefusedWhileEnding,
    #[error("this process cannot adopt the orphaned processes of its children")]
    BecomeSubreaper(#[source] Errno),
    #[error("the exit of the started process cannot be watched")]
    OpenExitEvent(#[source] Errno),
    #[error("the exit of the started process cannot be read")]
    Wait(#[source] Errno),
    #[error("the processes and pipes cannot be watched for their next event")]
    WaitForReadiness(#[source] Errno),
    #[error("the termination signals cannot be blocked for this process")]
    BlockSignals(#[source] nix::Error),
    #[error("the termination signals cannot be watched")]
    WatchSignals(#[source] nix::Error),
    #[error("the end of the watched work cannot be announced to the signal watcher")]
    CreateFinishEvent(#[source] Errno),
    #[error("the arrived termination signal cannot be read")]
    ReceiveSignal(#[source] nix::Error),
}
