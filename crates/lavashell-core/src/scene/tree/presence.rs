// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Composite, composite_fields};
use crate::constant;
use crate::geometry::{FinitePoint, Point, Rect, Scale};
use crate::scene::Transform;
use crate::units::{Fraction, ScaleFactor};

#[cfg(feature = "serde")]
mod encoding;

/// The point of a node about which its presence scales, as fractions of the width and the height of the node from its
/// top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScaleOrigin {
    pub x: Fraction,
    pub y: Fraction,
}

impl ScaleOrigin {
    pub const CENTER: Self = Self {
        x: constant!(Fraction::try_new(0.5)),
        y: constant!(Fraction::try_new(0.5)),
    };
    pub const BOTTOM: Self = Self {
        x: constant!(Fraction::try_new(0.5)),
        y: constant!(Fraction::try_new(1.0)),
    };

    fn of(self, bounds: Rect) -> Point {
        Point::new(
            bounds.x + bounds.width * self.x.get(),
            bounds.y + bounds.height * self.y.get(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "encoding::PresenceFields")
)]
pub struct Presence {
    opacity: f64,
    offset: Point,
    scale: ScaleFactor,
    origin: ScaleOrigin,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            offset: Point::default(),
            scale: ScaleFactor::ONE,
            origin: ScaleOrigin::CENTER,
        }
    }
}

impl Presence {
    #[must_use]
    pub fn fade() -> Self {
        Self {
            opacity: 0.0,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn opacity(mut self, opacity: Fraction) -> Self {
        self.opacity = opacity.get();
        self
    }

    #[must_use]
    pub fn slide(mut self, offset: FinitePoint) -> Self {
        self.offset = offset.get();
        self
    }

    #[must_use]
    pub fn scale(mut self, scale: ScaleFactor) -> Self {
        self.scale = scale;
        self
    }

    #[must_use]
    pub fn scale_origin(mut self, origin: ScaleOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub(crate) fn alpha(self) -> f64 {
        self.opacity
    }
}

composite_fields! {
    pub struct PresenceParts {
        pub opacity: f64,
        pub scale: ScaleFactor,
        pub offset: Point,
    }
    vectors PresenceVectors
}

impl Composite for Presence {
    type Parts = PresenceParts;

    fn parts(&self) -> PresenceParts {
        PresenceParts {
            opacity: self.opacity,
            scale: self.scale,
            offset: self.offset,
        }
    }

    fn with_parts(&self, parts: PresenceParts) -> Self {
        Self {
            opacity: parts.opacity.clamp(0.0, 1.0),
            offset: parts.offset,
            scale: parts.scale,
            origin: self.origin,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(
        into = "encoding::PresenceMapParts",
        try_from = "encoding::PresenceMapParts"
    )
)]
pub struct PresenceMap {
    /// Maps the coordinates in which the transition animates the bounds of the node into the coordinates of the
    /// parent of the group opened for the node.
    outer: Transform,
    /// Moves and scales the laid-out bounds of the node toward the animated bounds. `None` when the bounds of the
    /// node change at once, without an animation.
    stretch: Option<Stretch>,
    /// The laid-out bounds of the node, grown to the animated size. The scale of the presence applies about its
    /// origin in this rectangle.
    surface: Rect,
}

impl PresenceMap {
    pub(crate) fn new(outer: Transform, stretch: Option<Stretch>, surface: Rect) -> Self {
        Self {
            outer,
            stretch,
            surface,
        }
    }

    pub(crate) fn transform(&self, position: Point, presence: &Presence) -> Transform {
        let stretch = (self.stretch).map_or(Transform::IDENTITY, |stretch| stretch.at(position));
        Transform::translation(presence.offset)
            .then(self.outer.then(stretch))
            .then(Transform::scaled_about_point(
                presence.origin.of(self.surface),
                presence.scale.get(),
            ))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Stretch {
    pub from: Point,
    pub origin: Point,
    pub scale: Scale,
}

impl Stretch {
    pub fn at(self, position: Point) -> Transform {
        let to = position.offset(self.origin.x, self.origin.y);
        Transform::moving(self.from, to, self.scale)
    }
}
