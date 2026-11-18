// SPDX-License-Identifier: GPL-3.0-only
use crate::PointerPress;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::window::ResizeEdge;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowControlRequest {
    BeginMove {
        surface: SurfaceId,
        press: PointerPress,
    },
    BeginResize {
        surface: SurfaceId,
        press: PointerPress,
        edge: ResizeEdge,
    },
    SetMaximized {
        surface: SurfaceId,
        maximized: bool,
    },
    Minimize {
        surface: SurfaceId,
    },
    SetFullscreen {
        surface: SurfaceId,
        fullscreen: bool,
    },
    ShowMenu {
        surface: SurfaceId,
        press: PointerPress,
        at: Point,
    },
}

impl WindowControlRequest {
    #[must_use]
    pub const fn begins_pointer_grab(&self) -> bool {
        match self {
            Self::BeginMove { .. } | Self::BeginResize { .. } => true,
            Self::SetMaximized { .. }
            | Self::Minimize { .. }
            | Self::SetFullscreen { .. }
            | Self::ShowMenu { .. } => false,
        }
    }
}
