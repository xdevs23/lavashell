// SPDX-License-Identifier: GPL-3.0-only
//! The failures that the tests of this driver return in place of a panic.

use crate::error::SessionError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the session failed")]
    Session(#[from] SessionError),
    #[error("the keymap names no key")]
    NoNamedKey,
}
