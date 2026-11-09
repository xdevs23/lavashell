// SPDX-License-Identifier: GPL-3.0-only
//! The failures that surface record tests return in place of a panic.

#[derive(Debug, thiserror::Error)]
pub(crate) enum SurfaceRecordsTestError {
    #[error("the surface is not open")]
    NotOpen,
}
