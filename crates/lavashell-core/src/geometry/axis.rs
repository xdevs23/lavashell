// SPDX-License-Identifier: GPL-3.0-only
use super::{Constraints, Point, Rect, Scale, Size, Span};
use crate::units::Dp;

/// The direction along which a row or a column places its children, and along which a drag or a
/// scroll moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
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

    /// The point that lies `length` away from the origin along the axis.
    pub(crate) fn vector(self, length: Dp) -> Point {
        match self {
            Self::Horizontal => Point::new(length, Dp::ZERO),
            Self::Vertical => Point::new(Dp::ZERO, length),
        }
    }

    /// `size` with `length` along the axis.
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

    /// `constraints` with `span` along the axis.
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

    /// How far `to` is from `from` along the axis. The distance is negative when `to` lies before `from`.
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
