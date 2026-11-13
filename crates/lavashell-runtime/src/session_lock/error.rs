// SPDX-License-Identifier: GPL-3.0-only
/// The failures that can occur in the session lock.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the session-lock handle belongs to another lock")]
    Reused,
    #[error("a session lock needs an output")]
    WithoutOutput,
    #[error("the host denied or ended the session lock")]
    Ended,
}
