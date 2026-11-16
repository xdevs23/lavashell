// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, thiserror::Error)]
pub(crate) enum SurfaceRecordsTestError {
    #[error("the surface is not open")]
    NotOpen,
}
