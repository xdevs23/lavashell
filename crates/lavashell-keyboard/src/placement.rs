// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::{Corners, Dp, NonNegativeDp, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Placement {
    #[default]
    FullWidth,
    Docked(Dock),
}

impl Placement {
    pub(crate) fn panel(self, keyboard: Rect) -> Rect {
        let Self::Docked(dock) = self else {
            return keyboard;
        };
        let margin = dock.margin.get();
        let room = (keyboard.width - margin * 2.0).max(Dp::ZERO);
        let width = dock.width.get().min(room);
        let x = match dock.side {
            Side::Left => keyboard.x + margin,
            Side::Right => keyboard.x + keyboard.width - margin - width,
        };
        let height = (keyboard.height - margin).max(Dp::ZERO);
        Rect::new(x, keyboard.y, width, height)
    }

    pub(crate) fn corners(self, radius: Dp) -> Corners {
        match self {
            Self::FullWidth => Corners::new(radius, radius, Dp::ZERO, Dp::ZERO),
            Self::Docked(_) => Corners::all(radius),
        }
    }

    pub(crate) fn below(self) -> Dp {
        match self {
            Self::FullWidth => Dp::ZERO,
            Self::Docked(dock) => dock.margin.get(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dock {
    pub side: Side,
    pub width: NonNegativeDp,
    pub margin: NonNegativeDp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}
