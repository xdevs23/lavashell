// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Point;

/// One piece of a path, as a straight line or a Bézier curve. A piece starts where the piece before it ends, or at the
/// start of the path for the first piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line(Point),
    Quadratic {
        control: Point,
        to: Point,
    },
    Cubic {
        first_control: Point,
        second_control: Point,
        to: Point,
    },
}

impl Segment {
    /// The segment with `place` applied to each of its points.
    #[must_use]
    pub fn placed(self, place: impl Fn(Point) -> Point) -> Self {
        match self {
            Self::Line(to) => Self::Line(place(to)),
            Self::Quadratic { control, to } => Self::Quadratic {
                control: place(control),
                to: place(to),
            },
            Self::Cubic {
                first_control,
                second_control,
                to,
            } => Self::Cubic {
                first_control: place(first_control),
                second_control: place(second_control),
                to: place(to),
            },
        }
    }

    /// The control points of the segment, then the point where it ends.
    pub fn points(self) -> impl Iterator<Item = Point> {
        let (points, count) = match self {
            Self::Line(to) => ([to; 3], 1),
            Self::Quadratic { control, to } => ([control, to, to], 2),
            Self::Cubic {
                first_control,
                second_control,
                to,
            } => ([first_control, second_control, to], 3),
        };
        points.into_iter().take(count)
    }
}

/// The point where a path starts, then the points of its segments. Every curve of the path lies inside the bounding
/// box of these points.
pub(crate) fn path_points(start: Point, segments: &[Segment]) -> impl Iterator<Item = Point> + '_ {
    std::iter::once(start).chain(segments.iter().flat_map(|segment| segment.points()))
}
