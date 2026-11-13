// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PlatformRequest;

/// A failure of the platform of an AOSP host.
#[derive(Debug, thiserror::Error)]
pub enum AospError {
    #[error("the run was not started inside `AospHost::serve`")]
    Unserved,
    #[error(
        "the runtime requested the surface {requested:?} while {open:?} is open. An AOSP host shows one surface"
    )]
    SecondSurface {
        open: SurfaceId,
        requested: SurfaceId,
    },
    #[error("the host created a new Surface before it destroyed the previous one")]
    SecondWindow,
    #[error("cannot read the size of the native window of the host")]
    WindowSize(#[source] Error),
    #[error("an AOSP host does not support the request {0:?}")]
    Unsupported(Box<PlatformRequest>),
}
