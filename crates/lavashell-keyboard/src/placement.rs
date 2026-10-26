// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::{Corners, Dp, NonNegativeDp, Rect};

/// Where the panel sits at the bottom of the view in which the keyboard is composed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Placement {
    /// As wide as the viewport, with the top corners rounded.
    #[default]
    FullWidth,
    /// In a bottom corner, with four rounded corners.
    Docked(Dock),
}

impl Placement {
    /// The rectangle of the panel inside `keyboard`, the rectangle of the whole keyboard. A
    /// full-width panel fills it. A docked panel leaves the margin of the dock below and beside it.
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

    /// The corners of the panel at this placement, rounded by `radius`: the top corners across the
    /// whole width, or all four in a dock.
    pub(crate) fn corners(self, radius: Dp) -> Corners {
        match self {
            Self::FullWidth => Corners::new(radius, radius, Dp::ZERO, Dp::ZERO),
            Self::Docked(_) => Corners::all(radius),
        }
    }

    /// How far the keyboard extends below the panel.
    pub(crate) fn below(self) -> Dp {
        match self {
            Self::FullWidth => Dp::ZERO,
            Self::Docked(dock) => dock.margin.get(),
        }
    }
}

/// A panel `width` wide in the bottom corner at `side`, `margin` away from the bottom edge and from
/// the edge at `side`. A dock wider than the room between both margins is as wide as that room.
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
