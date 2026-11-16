// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Point, Rect, Scale, Size};
use crate::scene::Transform;
use crate::scene::tree::Stretch;
use crate::units::Dp;
use crate::view::transition::BoundsMotion;

#[derive(Clone, Copy)]
pub(in crate::view) struct Surfaces {
    pub stretch: Transform,
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
        let (scale, growth) = match motion {
            BoundsMotion::Instant => return (Self::STILL, None),
            BoundsMotion::Scale => (Scale::between(bounds.size(), live.size()), Size::ZERO),
            BoundsMotion::Reflow => (
                Transform::IDENTITY.scale,
                Size::new(live.width - bounds.width, live.height - bounds.height),
            ),
        };
        let stretch = Stretch {
            from: bounds.origin(),
            origin,
            scale,
        };
        let surfaces = Self {
            stretch: stretch.at(live.origin()),
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
