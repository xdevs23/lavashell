// SPDX-License-Identifier: GPL-3.0-only
use crate::watches;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
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

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Watch(error) => error.next_step(),
            Self::WatchPipe(_) | Self::ReserveTransfer(_) => NextStep::FreeSystemResources,
            Self::ReadPipe(_) | Self::WritePipe(_) => NextStep::RepeatTheTransfer,
        }
    }
}
