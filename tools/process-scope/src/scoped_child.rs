// SPDX-License-Identifier: GPL-3.0-only
use crate::error::ProcessError;
use crate::process_exit::ProcessExit;
use crate::process_group::ProcessGroup;
use rustix::io::Errno;
use rustix::process::{
    Pid, PidfdFlags, WaitId, WaitIdOptions, getpid, pidfd_open, set_child_subreaper, waitid,
};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

pub struct ScopedChild {
    group: ProcessGroup,
    exit_event: OwnedFd,
    child: Child,
}

impl ScopedChild {
    pub fn spawn(command: &mut Command) -> Result<Self, ProcessError> {
        set_child_subreaper(Some(getpid())).map_err(ProcessError::BecomeSubreaper)?;
        let (group, child) = ProcessGroup::start(command)?;
        let exit_event =
            pidfd_open(group.leader(), PidfdFlags::empty()).map_err(ProcessError::OpenExitEvent)?;
        Ok(Self {
            group,
            exit_event,
            child,
        })
    }

    #[must_use]
    pub fn pid(&self) -> Pid {
        self.group.leader()
    }

    #[must_use]
    pub fn exit_event(&self) -> BorrowedFd<'_> {
        self.exit_event.as_fd()
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub fn wait(&self) -> Result<ProcessExit, ProcessError> {
        let exited = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT;
        loop {
            match waitid(WaitId::PidFd(self.exit_event()), exited) {
                Ok(status) => {
                    if let Some(exit) = status.as_ref().and_then(ProcessExit::read) {
                        return Ok(exit);
                    }
                }
                Err(Errno::INTR) => {}
                Err(error) => return Err(ProcessError::Wait(error)),
            }
        }
    }
}
