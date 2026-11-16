// SPDX-License-Identifier: GPL-3.0-only
use crate::watches;
use std::collections::TryReserveError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the transfer pipe could not be watched")]
    WatchPipe(#[source] io::Error),
    #[error(transparent)]
    Watch(watches::Error),
    #[error("the transfer pipe could not be read")]
    ReadPipe(#[source] io::Error),
    #[error("the transfer pipe could not be written")]
    WritePipe(#[source] io::Error),
    #[error("the memory for the transfer could not be reserved")]
    ReserveTransfer(#[source] TryReserveError),
}
