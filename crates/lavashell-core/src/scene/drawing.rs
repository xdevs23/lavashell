// SPDX-License-Identifier: GPL-3.0-only
use super::{Affine, Brush, DrawCommand, DrawContext, Icon, Segment, TextStyle};
use crate::effects::{Backdrop, BlendMode, Material, Shadow, Stroke};
use crate::geometry::{Color, Point, Rect, Size};
use crate::image::{Image, ImageFit, StreamImage};
use crate::shape::{Corners, Shape};
use crate::units::{Dp, NonNegativeDp};

pub struct Drawing<'a> {
    pub commands: &'a mut Vec<DrawCommand>,
    pub bounds: Rect,
    pub context: &'a DrawContext,
}

impl<'a> Drawing<'a> {
    pub fn new(commands: &'a mut Vec<DrawCommand>, bounds: Rect, context: &'a DrawContext) -> Self {
        Self {
            commands,
            bounds,
            context,
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.bounds.size()
    }

    pub fn save(&mut self) {
        self.commands.push(DrawCommand::Save);
    }

    pub fn restore(&mut self) {
        self.commands.push(DrawCommand::Restore);
    }

    pub fn transform(&mut self, affine: Affine) {
        self.commands.push(DrawCommand::Transform(affine));
    }

    pub fn transformed(&mut self, affine: Affine, paint: impl FnOnce(&mut Self)) {
        self.save();
        self.transform(affine.translated(self.origin()));
        let previous = self.bounds;
        self.bounds.x = Dp::ZERO;
        self.bounds.y = Dp::ZERO;
        paint(self);
        self.bounds = previous;
        self.restore();
    }

    pub fn clip(&mut self, rect: Rect, corners: impl Into<Corners>) {
        let corners: Corners = corners.into();
        self.commands.push(DrawCommand::Clip {
            rect: self.absolute(rect),
            corners: corners.into(),
        });
    }

    /// Draws `image` into `destination` as `fit` places it, clipped by `destination`.
    pub fn image(&mut self, image: &Image, destination: Rect, fit: ImageFit) {
        let destination = self.absolute(destination);
        self.commands.push(DrawCommand::image(
            image,
            destination,
            fit,
            self.context.viewport.size,
        ));
    }

    /// Draws the stream frame `image` into `destination` as `fit` places it, clipped by `destination`.
    pub fn stream_image(&mut self, image: &StreamImage, destination: Rect, fit: ImageFit) {
        let destination = self.absolute(destination);
        let viewport = self.context.viewport.size;
        self.commands.push(DrawCommand::StreamImage {
            image: *image,
            destination,
            frame: fit.resolve(image.size, destination.size(), viewport),
        });
    }

    pub fn fill(&mut self, shape: impl Into<Shape>, brush: impl Into<Brush>) {
        let shape = self.shape(shape);
        let brush = self.brush(brush.into());
        self.commands.push(DrawCommand::Fill { shape, brush });
    }

    pub fn effect_layer(&mut self, rect: Rect, blend: BlendMode, blur: NonNegativeDp) {
        self.commands.push(DrawCommand::EffectLayer {
            bounds: self.absolute(rect),
            blend,
            blur,
            filter: None,
        });
    }

    pub fn backdrop(&mut self, shape: impl Into<Shape>, effect: Backdrop) {
        let shape = self.shape(shape);
        self.commands.push(DrawCommand::Backdrop { shape, effect });
    }

    pub fn shadow(&mut self, shape: impl Into<Shape>, shadow: Shadow) {
        let shape = self.shape(shape);
        self.commands.push(DrawCommand::Shadow { shape, shadow });
    }

    pub fn stroke(&mut self, shape: impl Into<Shape>, stroke: Stroke) {
        let shape = self.shape(shape);
        self.commands.push(DrawCommand::Stroke { shape, stroke });
    }

    pub fn material(&mut self, shape: impl Into<Shape>, material: &Material) {
        let origin = self.origin();
        let mut enclosing = Vec::new();
        material
            .commands(&self.shape(shape), origin)
            .paint(self.commands, &mut enclosing);
        self.commands.extend(enclosing.into_iter().rev());
    }

    pub fn circle(&mut self, center: Point, radius: Dp, brush: impl Into<Brush>) {
        let center = self.point(center);
        let brush = self.brush(brush.into());
        self.commands.push(DrawCommand::Circle {
            center,
            radius,
            brush,
        });
    }

    pub fn line(&mut self, from: Point, to: Point, width: Dp, color: Color) {
        self.path(&[from, to], false, color, Some(width));
    }

    /// Draws straight lines from each point in `points` to the next. An empty slice of points draws nothing.
    pub fn path(
        &mut self,
        points: &[Point],
        closed: bool,
        brush: impl Into<Brush>,
        stroke: Option<Dp>,
    ) {
        let Some((start, rest)) = points.split_first() else {
            return;
        };
        let start = self.point(*start);
        let segments = rest
            .iter()
            .map(|point| Segment::Line(self.point(*point)))
            .collect();
        let brush = self.brush(brush.into());
        self.commands.push(DrawCommand::Path {
            start,
            segments,
            closed,
            brush,
            stroke,
        });
    }

    /// Draws `icon` scaled into `square` in `color`. With `filled`, the closed paths of the icon are filled.
    pub fn icon(&mut self, icon: &Icon, square: Rect, color: Color, filled: bool) {
        icon.paint(self.absolute(square), color, filled, self.commands);
    }

    pub fn text(&mut self, text: impl Into<String>, rect: Rect, style: TextStyle) {
        let rect = self.absolute(rect);
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            rect,
            style: style.resolve(&self.context.text_scaling),
        });
    }

    fn origin(&self) -> Point {
        Point::new(self.bounds.x, self.bounds.y)
    }

    fn shape(&self, shape: impl Into<Shape>) -> Shape {
        shape.into().translated(self.origin())
    }

    fn point(&self, p: Point) -> Point {
        Point {
            x: self.bounds.x + p.x,
            y: self.bounds.y + p.y,
        }
    }

    fn absolute(&self, r: Rect) -> Rect {
        r.translated(self.origin())
    }

    fn brush(&self, brush: Brush) -> Brush {
        brush.translated(self.origin())
    }
}
