// SPDX-License-Identifier: GPL-3.0-only
use std::array::TryFromSliceError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub(crate) enum WireTestError {
    #[error("the socket could not be made non-blocking")]
    ConfigureSocket(#[source] io::Error),
    #[error("the requests of the client could not be read")]
    ReadProtocol(#[source] io::Error),
    #[error("a protocol word is shorter than four bytes")]
    ShortWord(#[from] TryFromSliceError),
}
