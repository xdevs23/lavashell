// SPDX-License-Identifier: GPL-3.0-only
use crate::PointerPress;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowControlRequest {
    BeginMove {
        surface: SurfaceId,
        press: PointerPress,
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
