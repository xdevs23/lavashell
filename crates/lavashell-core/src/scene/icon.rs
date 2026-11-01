// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawCommand, Segment, Transform, path_points};
use crate::geometry::{Color, Point, Rect, Scale, Size};
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
    /// icon as a whole through one opacity layer, so strokes that cross do not darken where they overlap. The paths
    /// are pushed with grid points under one transform, so the commands are the same wherever the icon is placed.
    pub(crate) fn paint(
        &self,
        square: Rect,
        color: Color,
        filled: bool,
        commands: &mut Vec<DrawCommand>,
    ) {
        let Some(placement) = self.placement(square) else {
            return;
        };
        let paths = self.grid_paths(color.with_alpha(1.0), filled);
        if paths.is_empty() {
            return;
        }
        let body = if color.alpha() < 1.0 {
            let widest = self
                .paths
                .iter()
                .filter_map(|path| path.stroke)
                .fold(0.0, f64::max);
            let layer = DrawCommand::OpacityLayer {
                alpha: color.alpha(),
                bounds: Rect::from_size(self.grid).outset(Dp::new(widest / 2.0)),
            };
            scoped([layer], paths)
        } else {
            paths
        };
        commands.extend(scoped(
            [
                DrawCommand::Save,
                DrawCommand::Transform(placement.affine()),
            ],
            body,
        ));
    }

    /// The transform that scales the grid to fit `square` and centers it there, or `None` when the grid or `square`
    /// has no area.
    fn placement(&self, square: Rect) -> Option<Transform> {
        let extends = |extent: Dp| extent.is_finite() && extent.get() > 0.0;
        if !extends(self.grid.width) || !extends(self.grid.height) {
            return None;
        }
        let scale = (square.width / self.grid.width).min(square.height / self.grid.height);
        (scale.is_finite() && scale > 0.0).then(|| Transform {
            scale: Scale { x: scale, y: scale },
            translation: Point::new(
                square.x + (square.width - self.grid.width * scale) / 2.0,
                square.y + (square.height - self.grid.height * scale) / 2.0,
            ),
        })
    }

    /// The path commands in `brush`, with points and stroke widths in grid units. A path with a point that is not
    /// finite is left out.
    fn grid_paths(&self, brush: Color, filled: bool) -> Vec<DrawCommand> {
        let finite = |path: &&IconPath| {
            path_points(path.start, path.segments)
                .all(|point| point.x.is_finite() && point.y.is_finite())
        };
        let mut paths = Vec::new();
        for path in self.paths.iter().filter(finite) {
            if path.stroke.is_none() || (filled && path.closed) {
                paths.push(DrawCommand::Path {
                    start: path.start,
                    segments: path.segments.to_vec(),
                    closed: true,
                    brush: brush.into(),
                    stroke: None,
                });
            }
            if let Some(width) = path.stroke {
                paths.push(DrawCommand::Path {
                    start: path.start,
                    segments: path.segments.to_vec(),
                    closed: path.closed,
                    brush: brush.into(),
                    stroke: Some(Dp::new(width)),
                });
            }
        }
        paths
    }
}

/// `inner` between `opening` and the `Restore` that closes the scope opened by `opening`.
fn scoped<const N: usize>(opening: [DrawCommand; N], inner: Vec<DrawCommand>) -> Vec<DrawCommand> {
    opening
        .into_iter()
        .chain(inner)
        .chain([DrawCommand::Restore])
        .collect()
}
