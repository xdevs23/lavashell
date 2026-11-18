// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PointerDevice;

#[derive(Debug, thiserror::Error)]
pub enum WindowControlError {
    #[error(
        "the runtime requested a window control for the pointer {0:?}, which does not belong to a seat"
    )]
    NoSeat(PointerDevice),
    #[error("the runtime requested a window control for the surface {0:?}, which is not a window")]
    NotAWindow(SurfaceId),
}
