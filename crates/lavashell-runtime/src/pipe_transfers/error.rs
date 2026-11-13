// SPDX-License-Identifier: GPL-3.0-only
use std::collections::TryReserveError;
use std::io;

/// The failures that can occur while moving the bytes of a transfer through a pipe.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the transfer pipe could not be watched")]
    WatchPipe(#[source] io::Error),
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("the transfer pipe could not be read")]
    ReadPipe(#[source] io::Error),
    #[error("the transfer pipe could not be written")]
    WritePipe(#[source] io::Error),
    #[error("the memory for the transfer could not be reserved")]
    ReserveTransfer(#[source] TryReserveError),
}
