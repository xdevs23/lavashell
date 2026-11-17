// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use lavashell_actor::LockPoisoned;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::NumberError;
use lavashell_platform::PlatformRequest;

#[derive(Debug, thiserror::Error)]
pub enum AospError {
    #[error("the host can execute only one run and has already executed it")]
    HostAlreadyServed,
    #[error("the host cannot use the lock of its channel")]
    HostChannelLock(#[from] LockPoisoned),
    #[error(
        "the runtime requested the surface {requested:?} while {open:?} is open. An AOSP host shows one surface"
    )]
    SecondSurface {
        open: SurfaceId,
        requested: SurfaceId,
    },
    #[error(
        "the runtime addressed the surface {requested:?} while {open:?} is the one this host has"
    )]
    UnknownSurface {
        open: Option<SurfaceId>,
        requested: SurfaceId,
    },
    #[error("the host created a new Surface before it destroyed the previous one")]
    SecondWindow,
    #[error("cannot read the size of the native window of the host")]
    WindowSize(#[source] Error),
    #[error("the layout size cannot fill the Surface")]
    FitLayout(#[source] NumberError),
    #[error("an AOSP host does not support the request {0:?}")]
    Unsupported(Box<PlatformRequest>),
}
