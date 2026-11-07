// SPDX-License-Identifier: GPL-3.0-only
use crate::Merge;

/// The state of an actor. The actor runs on its own thread and changes only through the inputs that arrive in its
/// mailbox.
pub trait Actor {
    type Input: Merge + Send + 'static;
    type Error: std::error::Error + Send + 'static;

    /// # Errors
    ///
    /// Fails when the actor cannot handle `input`. The actor then stops with the error.
    fn handle(&mut self, input: Self::Input) -> Result<Flow, Self::Error>;
}

/// Whether an actor handles more inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}
