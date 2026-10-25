// SPDX-License-Identifier: GPL-3.0-only
//! The shapes on which materials, backdrops, shadows and strokes are painted: rounded rectangles, polygons, unions
//! of shapes and rounded rectangles with a hole.

use crate::geometry::{ContourPoint, Point, Rect};
use crate::units::{Dp, NonNegativeDp, NumberError};
use std::borrow::Cow;

#[cfg(feature = "serde")]
mod encoding;
mod union;
pub use union::{Union, UnionMember};

/// The radii of the four corners of a rectangle, clockwise from the top left.
/// A radius of type `Dp` is circular. A `Radius` has a horizontal and a vertical length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Corners<R = Dp> {
    pub top_left: R,
    pub top_right: R,
    pub bottom_right: R,
    pub bottom_left: R,
}

impl<R: Copy> Corners<R> {
    pub const fn new(top_left: R, top_right: R, bottom_right: R, bottom_left: R) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    pub const fn all(radius: R) -> Self {
        Self::new(radius, radius, radius, radius)
    }

    pub fn to_array(self) -> [R; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    pub fn map<S: Copy>(self, f: impl Fn(R) -> S) -> Corners<S> {
        Corners::new(
            f(self.top_left),
            f(self.top_right),
            f(self.bottom_right),
            f(self.bottom_left),
        )
    }
}

impl Corners {
    /// The shared radius when all four corners are equal.
    #[must_use]
    pub fn uniform(self) -> Option<Dp> {
        let [first, rest @ ..] = self.to_array();
        rest.iter().all(|radius| *radius == first).then_some(first)
    }
}

impl From<Dp> for Corners {
    fn from(radius: Dp) -> Self {
        Self::all(radius)
    }
}

impl From<Corners> for Corners<Radius> {
    fn from(corners: Corners) -> Self {
        corners.map(Radius::circular)
    }
}

impl Corners<Radius> {
    #[must_use]
    pub fn circular(self) -> Option<Corners> {
        (self.to_array().iter().all(|radius| radius.x == radius.y))
            .then(|| self.map(|radius| radius.x))
    }

    /// Radii as drawn on `rect`: a radius that does not fit is scaled down
    /// until it fits half the width and half the height. A negative radius or
    /// one that is not a number draws a square corner, and a positive infinite
    /// one is as large as fits.
    #[must_use]
    pub fn fitted(self, rect: Rect) -> Self {
        let half = (
            rect.width.max(Dp::ZERO) / 2.0,
            rect.height.max(Dp::ZERO) / 2.0,
        );
        let longest = half.0.max(half.1);
        self.map(|radius| {
            let drawn = |r: Dp| {
                if r == Dp::INFINITY {
                    longest
                } else {
                    r.max(Dp::ZERO)
                }
            };
            let (x, y) = (drawn(radius.x), drawn(radius.y));
            let fit = |r: Dp, limit: Dp| if r > limit { limit / r } else { 1.0 };
            let scale = fit(x, half.0).min(fit(y, half.1));
            Radius::new(x * scale, y * scale)
        })
    }
}

/// A rectangle with elliptical corners whose radii are already fitted to the sides, as they are drawn.
#[derive(Clone, Copy)]
pub(crate) struct DrawnRoundedRect {
    rect: Rect,
    corners: Corners<Radius>,
}

impl DrawnRoundedRect {
    pub(crate) fn new(rect: Rect, corners: Corners<Radius>) -> Self {
        Self {
            rect,
            corners: corners.fitted(rect),
        }
    }

    pub(crate) fn rect(self) -> Rect {
        self.rect
    }

    pub(crate) fn corners(self) -> Corners<Radius> {
        self.corners
    }

    pub(crate) fn contains(self, point: Point) -> bool {
        let rect = self.rect;
        if !rect.contains(point) {
            return false;
        }
        let center = rect.center();
        let left = point.x < center.x;
        let top = point.y < center.y;
        let radius = match (left, top) {
            (true, true) => self.corners.top_left,
            (false, true) => self.corners.top_right,
            (false, false) => self.corners.bottom_right,
            (true, false) => self.corners.bottom_left,
        };
        if radius.x == Dp::ZERO || radius.y == Dp::ZERO {
            return true;
        }
        let dx = if left {
            rect.x + radius.x - point.x
        } else {
            point.x - (rect.x + rect.width - radius.x)
        };
        let dy = if top {
            rect.y + radius.y - point.y
        } else {
            point.y - (rect.y + rect.height - radius.y)
        };
        let past = |distance: Dp, radius: Dp| distance.max(Dp::ZERO) / radius;
        past(dx, radius.x).powi(2) + past(dy, radius.y).powi(2) <= 1.0
    }
}

/// The horizontal and vertical radius of an elliptical corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Radius {
    pub x: Dp,
    pub y: Dp,
}

impl Radius {
    #[must_use]
    pub const fn new(x: Dp, y: Dp) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub const fn circular(radius: Dp) -> Self {
        Self::new(radius, radius)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoundedRect {
    pub rect: Rect,
    pub corners: Corners,
}

impl RoundedRect {
    #[must_use]
    pub const fn new(rect: Rect, corners: Corners) -> Self {
        Self { rect, corners }
    }

    /// Radii as drawn: each one between zero and half the shorter side.
    #[must_use]
    pub fn fitted_corners(&self) -> Corners {
        self.drawn().corners().map(|radius| radius.x)
    }

    /// Whether `point` lies inside this rectangle with its corners as drawn.
    #[must_use]
    pub fn contains(&self, point: Point) -> bool {
        self.drawn().contains(point)
    }

    fn drawn(&self) -> DrawnRoundedRect {
        DrawnRoundedRect::new(self.rect, self.corners.into())
    }

    /// Grows every side and radius by `amount`, or shrinks them for a negative amount. A radius does
    /// not shrink below zero.
    #[must_use]
    pub fn outset(self, amount: Dp) -> Self {
        Self::new(
            self.rect.outset(amount),
            self.corners.map(|radius| (radius + amount).max(Dp::ZERO)),
        )
    }

    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        Self::new(self.rect.translated(offset), self.corners)
    }

    /// This rectangle with `hole` cut out of it. The hole may extend beyond
    /// this rectangle or cover all of it. A hole without a positive width and
    /// height cuts nothing, so the result is this rectangle.
    #[must_use]
    pub fn minus(self, hole: RoundedRect) -> Shape {
        Hollow::new(self, hole).map_or_else(|| self.into(), Shape::Hollow)
    }
}

/// A rounded rectangle with a rounded rectangle cut out of it. In a union,
/// the hole is open wherever no other member covers it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "encoding::HollowParts")
)]
pub struct Hollow {
    outer: RoundedRect,
    hole: RoundedRect,
}

impl Hollow {
    /// `None` when the hole has no positive width and height, so that it cuts nothing.
    fn new(outer: RoundedRect, hole: RoundedRect) -> Option<Self> {
        let cuts = hole.rect.width > Dp::ZERO && hole.rect.height > Dp::ZERO;
        cuts.then_some(Self { outer, hole })
    }

    #[must_use]
    pub fn outer(&self) -> RoundedRect {
        self.outer
    }

    #[must_use]
    pub fn hole(&self) -> RoundedRect {
        self.hole
    }

    fn translated(&self, offset: Point) -> Self {
        Self {
            outer: self.outer.translated(offset),
            hole: self.hole.translated(offset),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PointsError {
    #[error("a polygon needs at least three points and a contour at least two")]
    TooFew,
    #[error("the points of a shape must be finite")]
    NotFinite,
}

/// Fails when `points` has fewer than `minimum` points or one that is not finite.
pub(crate) fn check_points(
    points: &[impl ContourPoint],
    minimum: usize,
) -> Result<(), PointsError> {
    if points.len() < minimum {
        Err(PointsError::TooFew)
    } else if points
        .iter()
        .any(|p| !p.coordinates().iter().all(|c| c.is_finite()))
    {
        Err(PointsError::NotFinite)
    } else {
        Ok(())
    }
}

/// A closed polygon of at least three finite points, filled by the even-odd rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    points: Vec<Point>,
    bounds: Rect,
}

impl Polygon {
    /// # Errors
    ///
    /// Fails with fewer than three points or a point that is not finite.
    pub fn new(points: impl Into<Vec<Point>>) -> Result<Self, PointsError> {
        let points = points.into();
        check_points(&points, 3)?;
        let bounds = Rect::bounding(points.iter().copied()).ok_or(PointsError::TooFew)?;
        Ok(Self { points, bounds })
    }

    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    fn translated(&self, offset: Point) -> Self {
        Self {
            points: (self.points.iter())
                .map(|p| p.offset(offset.x, offset.y))
                .collect(),
            bounds: self.bounds.translated(offset),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Shape {
    Rounded(RoundedRect),
    Contour(Polygon),
    Union(Union),
    Hollow(Hollow),
}

impl From<RoundedRect> for Shape {
    fn from(shape: RoundedRect) -> Self {
        Self::Rounded(shape)
    }
}

impl From<Rect> for Shape {
    fn from(rect: Rect) -> Self {
        Self::Rounded(RoundedRect::new(rect, Corners::default()))
    }
}

impl Shape {
    #[must_use]
    pub fn rounded(rect: Rect, radius: Dp) -> Self {
        Self::Rounded(RoundedRect::new(rect, Corners::all(radius)))
    }

    /// The union of `members`, or `None` when `members` is empty.
    pub fn union(
        members: impl IntoIterator<Item = UnionMember>,
        spacing: NonNegativeDp,
    ) -> Option<Self> {
        Union::new(members.into_iter().collect(), spacing).map(Self::Union)
    }

    /// The bounds of the outline, which for a shape with a hole are the
    /// bounds of its outer rectangle.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rounded(shape) => shape.rect,
            Self::Contour(shape) => shape.bounds(),
            Self::Union(shape) => shape.bounds(),
            Self::Hollow(shape) => shape.outer.rect,
        }
    }

    /// The closed outline as a polygon. Rounded corners are followed to within half of `device_pixel`. `None` for
    /// a union and for a shape with a hole.
    ///
    /// # Errors
    ///
    /// Fails when a radius is not finite.
    pub fn outline(&self, device_pixel: Dp) -> Result<Option<Vec<Point>>, NumberError> {
        match self {
            Self::Rounded(shape) => shape.outline(device_pixel).map(Some),
            Self::Contour(shape) => Ok(Some(shape.points.clone())),
            Self::Union(_) | Self::Hollow(_) => Ok(None),
        }
    }

    #[must_use]
    pub fn translated(&self, offset: Point) -> Self {
        match self {
            Self::Rounded(shape) => Self::Rounded(shape.translated(offset)),
            Self::Contour(shape) => Self::Contour(shape.translated(offset)),
            Self::Union(shape) => Self::Union(shape.translated(offset)),
            Self::Hollow(shape) => Self::Hollow(shape.translated(offset)),
        }
    }

    /// This shape with every hole filled, borrowed when it has none.
    #[must_use]
    pub fn without_holes(&self) -> Cow<'_, Self> {
        match self {
            Self::Rounded(_) | Self::Contour(_) => Cow::Borrowed(self),
            Self::Hollow(shape) => Cow::Owned(shape.outer.into()),
            Self::Union(union) => (union.without_holes())
                .map_or(Cow::Borrowed(self), |union| Cow::Owned(Self::Union(union))),
        }
    }
}

mod outline;

#[cfg(test)]
mod tests;
