// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawCommand, Segment, path_points};
use crate::geometry::{Color, Point, Rect, Size};
use crate::units::Dp;

/// An icon drawn from paths. The points of the paths lie on a grid that is `grid.width` by `grid.height` units.
#[derive(Debug, PartialEq)]
pub struct Icon {
    pub grid: Size,
    pub paths: &'static [IconPath],
}

/// A path from `start` through its segments. A path without a stroke width is filled.
#[derive(Debug, PartialEq)]
pub struct IconPath {
    pub start: Point,
    pub segments: &'static [Segment],
    pub closed: bool,
    /// The width of the stroke, in units of the grid.
    pub stroke: Option<f64>,
}

impl Icon {
    /// Scales the grid to fit `square`, centers it there and pushes draw commands for the paths in `color` to
    /// `commands`. With `filled`, closed paths with a stroke are filled as well. A translucent color applies to the
    /// icon as a whole through one opacity layer, so strokes that cross do not darken where they overlap.
    pub(crate) fn paint(
        &self,
        square: Rect,
        color: Color,
        filled: bool,
        commands: &mut Vec<DrawCommand>,
    ) {
        let extends = |extent: Dp| extent.is_finite() && extent.get() > 0.0;
        if !extends(self.grid.width) || !extends(self.grid.height) {
            return;
        }
        let scale = (square.width / self.grid.width).min(square.height / self.grid.height);
        let left = square.x + (square.width - self.grid.width * scale) / 2.0;
        let top = square.y + (square.height - self.grid.height * scale) / 2.0;
        let translucent = color.alpha() < 1.0;
        if translucent {
            let widest = self
                .paths
                .iter()
                .filter_map(|path| path.stroke)
                .fold(0.0, f64::max);
            commands.push(DrawCommand::OpacityLayer {
                alpha: color.alpha(),
                bounds: square.outset(Dp::new(widest * scale / 2.0)),
            });
        }
        let brush = color.with_alpha(1.0);
        let place = |point: Point| Point::new(left + point.x * scale, top + point.y * scale);
        let finite = |path: &&IconPath| {
            path_points(path.start, path.segments)
                .all(|point| point.x.is_finite() && point.y.is_finite())
        };
        for path in self.paths.iter().filter(finite) {
            let start = place(path.start);
            let segments: Vec<Segment> = (path.segments.iter())
                .map(|segment| segment.placed(place))
                .collect();
            if path.stroke.is_none() || (filled && path.closed) {
                commands.push(DrawCommand::Path {
                    start,
                    segments: segments.clone(),
                    closed: true,
                    brush: brush.into(),
                    stroke: None,
                });
            }
            if let Some(width) = path.stroke {
                commands.push(DrawCommand::Path {
                    start,
                    segments,
                    closed: path.closed,
                    brush: brush.into(),
                    stroke: Some(Dp::new(width * scale)),
                });
            }
        }
        if translucent {
            commands.push(DrawCommand::Restore);
        }
    }
}
