// SPDX-License-Identifier: GPL-3.0-only
use super::{Constraints, Point, Rect, Scale, Size, Span};
use crate::units::Dp;
use lavashell_enum_values::enum_with_values;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum Axis {
        Horizontal,
        Vertical,
    }

    pub const ALL;
}

impl Axis {
    #[must_use]
    pub fn cross(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    #[must_use]
    pub fn coordinate(self, point: Point) -> Dp {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }

    #[must_use]
    pub fn length(self, size: Size) -> Dp {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }

    pub(crate) fn vector(self, length: Dp) -> Point {
        match self {
            Self::Horizontal => Point::new(length, Dp::ZERO),
            Self::Vertical => Point::new(Dp::ZERO, length),
        }
    }

    pub(crate) fn with_length(self, size: Size, length: Dp) -> Size {
        match self {
            Self::Horizontal => Size::new(length, size.height),
            Self::Vertical => Size::new(size.width, length),
        }
    }

    pub(crate) fn span(self, constraints: Constraints) -> Span {
        match self {
            Self::Horizontal => constraints.width,
            Self::Vertical => constraints.height,
        }
    }

    pub(crate) fn with_span(self, constraints: Constraints, span: Span) -> Constraints {
        match self {
            Self::Horizontal => Constraints {
                width: span,
                ..constraints
            },
            Self::Vertical => Constraints {
                height: span,
                ..constraints
            },
        }
    }

    pub(crate) fn start(self, rect: Rect) -> Dp {
        self.coordinate(Point::new(rect.x, rect.y))
    }

    pub(crate) fn end(self, rect: Rect) -> Dp {
        self.start(rect) + self.length(rect.size())
    }

    pub(crate) fn signed_distance(self, from: Point, to: Point) -> Dp {
        self.coordinate(to) - self.coordinate(from)
    }

    pub(crate) fn factor(self, scale: Scale) -> f64 {
        match self {
            Self::Horizontal => scale.x,
            Self::Vertical => scale.y,
        }
    }
}
