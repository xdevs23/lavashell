// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers;
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::transfer::Cancellation;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::{DragReceiveRefusal, DragRefusal};
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new drag could not get an id")]
    NumberDragSource(#[source] IdsExhausted),
    #[error("the platform did not start the drag")]
    StartDrag(#[source] DragRefusal),
    #[error("a drag needs a supported action")]
    WithoutAction,
    #[error("the destination asked for a format that the drag did not offer")]
    UnadvertisedFormat,
    #[error("the drag finished without a transfer and a supported action")]
    WithoutTransfer,
    #[error("the dragged data could not be sent")]
    SendDrag(#[source] io::Error),
    #[error("the dragged data could not be received")]
    ReceiveDrag(#[source] DragReceiveRefusal),
    #[error(transparent)]
    Transfer(#[from] pipe_transfers::Error),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::StartDrag(refusal) => refusal.next_step(),
            Self::ReceiveDrag(refusal) => refusal.next_step(),
            Self::Transfer(error) => error.next_step(),
            Self::Cancelled(cancellation) => cancellation.next_step(),
            Self::WithoutTransfer | Self::SendDrag(_) => NextStep::RepeatTheTransfer,
            Self::NumberDragSource(_) | Self::WithoutAction | Self::UnadvertisedFormat => {
                NextStep::ReportTheFailure
            }
        }
    }
}
