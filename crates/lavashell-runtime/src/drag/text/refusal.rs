// SPDX-License-Identifier: GPL-3.0-only
use crate::drag::Error;
use std::string::FromUtf8Error;

pub(in crate::drag) enum DropAccepted {
    Inserted,
    Moved,
}

#[derive(Debug, thiserror::Error)]
pub(in crate::drag) enum DropRefusal {
    #[error("the transfer does not belong to a drop that waits for its text")]
    NotPending,
    #[error("the dropped text was not received")]
    NotReceived(#[source] Error),
    #[error("the dropped text is not UTF-8")]
    NotUtf8(#[source] FromUtf8Error),
    #[error("the drop did not land on a text field")]
    NoTarget,
    #[error("the field that the moved text came from did not give it up")]
    MoveRefused,
    #[error("several selections could be the source of the moved text")]
    AmbiguousSource,
    #[error("the text field does not accept the dropped text at the drop position")]
    InsertRefused,
}
