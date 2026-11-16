// SPDX-License-Identifier: GPL-3.0-only
use crate::error::ProcessError;
use crate::process_group::end_live_groups_and_refuse_new_ones;
use crate::readiness::first_ready;
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use rustix::event::{EventfdFlags, eventfd};
use std::fmt;
use std::os::fd::{AsFd, OwnedFd};
use std::process::ExitCode;

const TERMINATION_SIGNALS: [Signal; 3] = [Signal::SIGHUP, Signal::SIGINT, Signal::SIGTERM];
// https://www.gnu.org/software/bash/manual/html_node/Exit-Status.html
const SIGNALLED_EXIT_BASE: i32 = 128;

pub enum RunOutcome<T> {
    Finished(T),
    Terminated(Termination),
}

#[derive(Debug, Clone, Copy)]
pub struct Termination(Signal);

impl Termination {
    #[must_use]
    pub fn exit_code(self) -> ExitCode {
        u8::try_from(self.exit_status()).map_or(ExitCode::FAILURE, ExitCode::from)
    }

    fn exit_status(self) -> i32 {
        SIGNALLED_EXIT_BASE + self.0 as i32
    }
}

impl fmt::Display for Termination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the run stopped the processes it started and ended on {}",
            self.0
        )
    }
}

#[derive(Clone, Copy)]
enum Wake {
    Signal,
    Finished,
}

pub fn run_ending_groups_on_termination<T>(
    body: impl FnOnce() -> T,
) -> Result<RunOutcome<T>, ProcessError> {
    let signals: SigSet = TERMINATION_SIGNALS.into_iter().collect();
    signals.thread_block().map_err(ProcessError::BlockSignals)?;
    let arrived = SignalFd::with_flags(&signals, SfdFlags::SFD_CLOEXEC)
        .map_err(ProcessError::WatchSignals)?;
    let finished = eventfd(0, EventfdFlags::CLOEXEC).map_err(ProcessError::CreateFinishEvent)?;
    std::thread::scope(|scope| {
        let watcher = scope.spawn(|| end_groups_on_signal(&signals, &arrived, &finished));
        let announcement = FinishAnnouncement(&finished);
        let result = body();
        drop(announcement);
        let termination = watcher
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))?;
        Ok(termination.map_or(RunOutcome::Finished(result), RunOutcome::Terminated))
    })
}

fn end_groups_on_signal(
    signals: &SigSet,
    arrived: &SignalFd,
    finished: &OwnedFd,
) -> Result<Option<Termination>, ProcessError> {
    let Some(first) = next_signal(signals, arrived, finished)? else {
        return Ok(None);
    };
    if let Err(error) = end_live_groups_and_refuse_new_ones() {
        eprintln!("the started processes did not all end on {first}: {error}");
    }
    if let Some(second) = next_signal(signals, arrived, finished)? {
        eprintln!("{second} arrived while this program cleaned up after {first}, so it exits now");
        std::process::exit(Termination(second).exit_status());
    }
    Ok(Some(Termination(first)))
}

fn next_signal(
    signals: &SigSet,
    arrived: &SignalFd,
    finished: &OwnedFd,
) -> Result<Option<Signal>, ProcessError> {
    match first_ready(&[
        (arrived.as_fd(), Wake::Signal),
        (finished.as_fd(), Wake::Finished),
    ])? {
        Wake::Finished => Ok(None),
        Wake::Signal => signals
            .wait()
            .map(Some)
            .map_err(ProcessError::ReceiveSignal),
    }
}

struct FinishAnnouncement<'a>(&'a OwnedFd);

impl Drop for FinishAnnouncement<'_> {
    fn drop(&mut self) {
        if let Err(error) = rustix::io::write(self.0, &1_u64.to_ne_bytes()) {
            eprintln!("the signal watcher cannot be told that the work finished: {error}");
        }
    }
}
