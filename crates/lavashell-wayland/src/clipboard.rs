// SPDX-License-Identifier: GPL-3.0-only

mod drag;
mod error;
pub(crate) mod pipes;
mod state;

pub(crate) use drag::portable_action;
pub use error::DragSourceError;
pub(crate) use pipes::into_nonblocking;
pub(crate) use state::ClipboardState;
