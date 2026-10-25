// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, Point, Rect, Scale, Size, Transform};
use crate::scene::tree::Stretch;
use crate::view::transition::BoundsMotion;

/// Where the material, background, border, clips and the other modifier and state rectangles of a
/// node paint while a transition moves its bounds. Each rectangle is laid out with the node, grows
/// by `growth` and paints under the transform of the content.
#[derive(Clone, Copy)]
pub(in crate::view) struct Surfaces {
    /// The part of the content transform that maps the laid-out bounds toward the animated bounds.
    pub stretch: Transform,
    /// The size of the animated bounds minus the size of the laid-out bounds.
    growth: Size,
}

impl Surfaces {
    pub const STILL: Self = Self {
        stretch: Transform::IDENTITY,
        growth: Size::ZERO,
    };

    pub fn new(
        motion: BoundsMotion,
        bounds: Rect,
        live: Rect,
        origin: Point,
    ) -> (Self, Option<Stretch>) {
        let ratio = |to: Dp, from: Dp| if from > Dp::ZERO { to / from } else { 1.0 };
        let (scale, growth) = match motion {
            BoundsMotion::Instant => return (Self::STILL, None),
            BoundsMotion::Scale => {
                let scale = Scale {
                    x: ratio(live.width, bounds.width),
                    y: ratio(live.height, bounds.height),
                };
                (scale, Size::ZERO)
            }
            BoundsMotion::Reflow => (
                Transform::IDENTITY.scale,
                Size::new(live.width - bounds.width, live.height - bounds.height),
            ),
        };
        let stretch = Stretch {
            from: Point::new(bounds.x, bounds.y),
            origin,
            scale,
        };
        let surfaces = Self {
            stretch: stretch.at(Point::new(live.x, live.y)),
            growth,
        };
        (surfaces, Some(stretch))
    }

    pub fn rect(self, rect: Rect) -> Rect {
        Rect::new(
            rect.x,
            rect.y,
            (rect.width + self.growth.width).max(Dp::ZERO),
            (rect.height + self.growth.height).max(Dp::ZERO),
        )
    }

    /// The rectangle at which `rect` paints, where the application placed `rect` inside the surface laid out at
    /// `layer`. A right or bottom edge of `rect` that lies on the edge of `layer` moves with the edge of the surface.
    /// Any other right or bottom edge remains in place, cut to the surface. The left and top edges do not move.
    pub fn placed(self, rect: Rect, layer: Rect) -> Rect {
        let surface = self.rect(layer);
        let edge = |start: Dp, extent: Dp, layer_end: Dp, surface_end: Dp| {
            let end = start + extent;
            if end == layer_end {
                surface_end
            } else {
                end.min(surface_end)
            }
        };
        let right = edge(
            rect.x,
            rect.width,
            layer.x + layer.width,
            surface.x + surface.width,
        );
        let bottom = edge(
            rect.y,
            rect.height,
            layer.y + layer.height,
            surface.y + surface.height,
        );
        Rect::new(
            rect.x,
            rect.y,
            (right - rect.x).max(Dp::ZERO),
            (bottom - rect.y).max(Dp::ZERO),
        )
    }
}
