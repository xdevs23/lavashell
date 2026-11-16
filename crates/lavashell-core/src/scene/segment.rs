// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Point;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

pub(crate) fn path_points(start: Point, segments: &[Segment]) -> impl Iterator<Item = Point> + '_ {
    std::iter::once(start).chain(segments.iter().flat_map(|segment| segment.points()))
}
