// SPDX-License-Identifier: GPL-3.0-only
use crate::checked_command::CommandLine;
use crate::error::ProcessError;
use rustix::io::Errno;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::{Mutex, PoisonError};

static REGISTRY: Mutex<GroupRegistry> = Mutex::new(GroupRegistry::open());

enum Admission {
    Open,
    Closed,
}

struct GroupRegistry {
    live: Vec<Pid>,
    admission: Admission,
}

impl GroupRegistry {
    const fn open() -> Self {
        Self {
            live: Vec::new(),
            admission: Admission::Open,
        }
    }
}

pub struct ProcessGroup(Pid);

impl ProcessGroup {
    pub fn start(command: &mut Command) -> Result<(Self, Child), ProcessError> {
        registry(|registry| match registry.admission {
            Admission::Closed => Err(ProcessError::RefusedWhileEnding),
            Admission::Open => {
                let child =
                    (command.process_group(0).spawn()).map_err(|source| ProcessError::Spawn {
                        command: CommandLine::from(&*command),
                        source,
                    })?;
                let leader = Pid::from_child(&child);
                registry.live.push(leader);
                Ok((Self(leader), child))
            }
        })
    }

    pub fn leader(&self) -> Pid {
        self.0
    }

    fn end(&self) -> Result<(), Errno> {
        registry(|registry| {
            registry.live.retain(|&group| group != self.0);
            kill(self.0)
        })?;
        reap(self.0)
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Err(error) = self.end() {
            eprintln!(
                "the processes in group {} did not end: {error}",
                self.0.as_raw_nonzero()
            );
        }
    }
}

pub fn end_live_groups_and_refuse_new_ones() -> Result<(), Errno> {
    registry(|registry| {
        registry.admission = Admission::Closed;
        registry
            .live
            .iter()
            .map(|&group| kill(group))
            .fold(Ok(()), Result::and)
    })
}

fn registry<T>(change: impl FnOnce(&mut GroupRegistry) -> T) -> T {
    change(&mut REGISTRY.lock().unwrap_or_else(PoisonError::into_inner))
}

fn kill(group: Pid) -> Result<(), Errno> {
    match kill_process_group(group, Signal::KILL) {
        Err(Errno::SRCH) => Ok(()),
        killed => killed,
    }
}

fn reap(group: Pid) -> Result<(), Errno> {
    loop {
        match waitid(WaitId::Pgid(Some(group)), WaitIdOptions::EXITED) {
            Ok(_reaped_member) => {}
            Err(Errno::INTR) => {}
            Err(Errno::CHILD) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}
