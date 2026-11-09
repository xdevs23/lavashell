// SPDX-License-Identifier: GPL-3.0-only
//! The clipboard selection and drag and drop between the app and other Wayland clients. The runtime negotiates each
//! transfer and moves its bytes through pipes. This module reports what the data devices do to the runtime and
//! executes the requests made by it.

mod drag;
mod pipes;
mod selection;
mod sources;
mod state;

pub(super) use state::ClipboardState;
