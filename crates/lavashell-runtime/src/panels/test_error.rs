// SPDX-License-Identifier: GPL-3.0-only
//! The failures that panel tests return in place of a panic.

#[derive(Debug, thiserror::Error)]
pub(crate) enum PanelsTestError {
    #[error("the surface has no panel")]
    NoPanel,
}
