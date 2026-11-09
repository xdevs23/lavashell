// SPDX-License-Identifier: GPL-3.0-only
use rustix::io::Errno;
use std::num::TryFromIntError;

/// The failures that can occur while the views wait for an input or a deadline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the file descriptor that wakes the views for an input could not be created")]
    CreateInputWake(#[source] Errno),
    #[error("a sender could not wake the views for an input")]
    SignalInput(#[source] Errno),
    #[error("waiting for an input failed")]
    WaitForInput(#[source] Errno),
    #[error("the deadline for an input is too far ahead for the system")]
    InputDeadline(#[source] TryFromIntError),
}
