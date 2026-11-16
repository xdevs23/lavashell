// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_platform::PointerDevice;

#[derive(Debug, thiserror::Error)]
pub enum WindowControlError {
    #[error("the runtime requested a window move for the pointer {0:?}, which belongs to no seat")]
    NoSeat(PointerDevice),
    #[error("the runtime requested a window move for the surface {0:?}, which is not a window")]
    NotAWindow(SurfaceId),
}
