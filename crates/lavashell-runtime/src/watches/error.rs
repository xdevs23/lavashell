// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::transfer::Cancellation;

/// The failures that can occur while asking the platform to watch a file descriptor.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}
