// SPDX-License-Identifier: GPL-3.0-only
mod footprint;

pub use footprint::Footprint;

use crate::geometry::{Point, Rect};
use crate::scene::{Affine, DrawCommand};
use crate::units::Dp;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    Within(Rect),
    Everywhere,
}

impl From<Rect> for Extent {
    fn from(rect: Rect) -> Self {
        let edges = [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ];
        if edges.iter().all(|edge| edge.is_finite()) {
            Self::Within(rect)
        } else {
            Self::Everywhere
        }
    }
}

impl Extent {
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Within(a), Self::Within(b)) => a.union(b).into(),
            _ => Self::Everywhere,
        }
    }

    #[must_use]
    pub fn outset(self, amount: Dp) -> Self {
        match self {
            Self::Within(rect) => rect.outset(amount).into(),
            Self::Everywhere => Self::Everywhere,
        }
    }

    #[must_use]
    pub fn mapped(self, affine: Affine) -> Self {
        let Self::Within(rect) = self else {
            return Self::Everywhere;
        };
        let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
        let corners = [
            (rect.x, rect.y),
            (right, rect.y),
            (right, bottom),
            (rect.x, bottom),
        ]
        .map(|(x, y)| affine.map_point(Point::new(x, y)));
        if !corners.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Self::Everywhere;
        }
        Rect::bounding(corners).map_or(Self::Everywhere, Self::from)
    }

    #[must_use]
    pub fn within(self, area: Rect) -> Option<Rect> {
        match self {
            Self::Within(rect) => rect.intersection(area),
            Self::Everywhere => Some(area),
        }
    }

    #[must_use]
    pub fn inside(self, area: Rect) -> bool {
        matches!(self, Self::Within(rect) if rect.x >= area.x
            && rect.y >= area.y
            && rect.x + rect.width <= area.x + area.width
            && rect.y + rect.height <= area.y + area.height)
    }
}
