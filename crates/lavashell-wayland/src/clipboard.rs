// SPDX-License-Identifier: GPL-3.0-only
//! The clipboard selection and drag and drop between the app and other Wayland clients. The runtime moves the bytes
//! of the selection through pipes. Drag and drop picks a MIME type from the types offered by the other side and moves
//! the data through nonblocking pipes watched by the event loop.

mod drag;
mod pipes;
mod selection;
mod sources;
mod state;
#[cfg(test)]
#[path = "text_transfer/tests.rs"]
mod text_transfer_tests;

pub(crate) use drag::DragCompletion;
pub use drag::{DragEvent, DragGesture, DragOfferInfo, DragRequest, DropEvent, TransferId};
pub(super) use state::ClipboardState;
