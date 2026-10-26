// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub x: Dp,
    pub y: Dp,
}

impl Point {
    #[must_use]
    pub const fn new(x: Dp, y: Dp) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn offset(self, x: Dp, y: Dp) -> Self {
        Self::new(self.x + x, self.y + y)
    }

    /// The dp point at physical pixel `x`, `y` of a surface with `density`.
    #[must_use]
    pub fn from_px(x: Px, y: Px, density: Density) -> Self {
        Self::new(density.to_dp(x), density.to_dp(y))
    }
}

/// A point in plain numbers, such as fractions of a radius, before it is placed in dp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlainPoint {
    pub x: f64,
    pub y: f64,
}

impl PlainPoint {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::Point {}

    impl Sealed for super::PlainPoint {}
}

/// A sample point of a `Contour`, the outline animated by a shape morph: a `Point` in dp or a `PlainPoint`.
pub trait ContourPoint: sealed::Sealed + Copy + PartialEq + 'static {
    fn coordinates(self) -> [f64; 2];

    fn from_coordinates(coordinates: [f64; 2]) -> Self;

    /// The smallest change of a coordinate that shows on a screen whose device pixel measures `pixel`.
    fn visible_step(pixel: Dp) -> f64;
}

impl ContourPoint for Point {
    fn coordinates(self) -> [f64; 2] {
        [self.x.get(), self.y.get()]
    }

    fn from_coordinates([x, y]: [f64; 2]) -> Self {
        Self::new(Dp::new(x), Dp::new(y))
    }

    fn visible_step(pixel: Dp) -> f64 {
        <Dp as crate::animation::Animatable>::visible_step(pixel)
    }
}

impl ContourPoint for PlainPoint {
    fn coordinates(self) -> [f64; 2] {
        [self.x, self.y]
    }

    fn from_coordinates([x, y]: [f64; 2]) -> Self {
        Self::new(x, y)
    }

    /// One step of a color channel at the highest bit depth of any output. The coordinates of a plain point are
    /// fractions, such as fractions of a radius, and change what shows by such steps.
    fn visible_step(_pixel: Dp) -> f64 {
        finest_step()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FinitePoint {
    pub x: FiniteDp,
    pub y: FiniteDp,
}

impl FinitePoint {
    /// # Errors
    ///
    /// Fails when a coordinate is not finite.
    pub const fn try_new(point: Point) -> Result<Self, NumberError> {
        match (FiniteDp::try_new(point.x), FiniteDp::try_new(point.y)) {
            (Ok(x), Ok(y)) => Ok(Self { x, y }),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }

    #[must_use]
    pub const fn get(self) -> Point {
        Point::new(self.x.get(), self.y.get())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Size {
    pub width: Dp,
    pub height: Dp,
}

impl Size {
    pub const ZERO: Self = Self::new(Dp::ZERO, Dp::ZERO);

    #[must_use]
    pub const fn new(width: Dp, height: Dp) -> Self {
        Self { width, height }
    }

    /// The dp size of `width` by `height` physical pixels of a surface with `density`.
    #[must_use]
    pub fn from_px(width: Px, height: Px, density: Density) -> Self {
        Self::new(density.to_dp(width), density.to_dp(height))
    }

    /// The width and the height in whole physical pixels, rounded up, of a buffer that covers this size at
    /// `density`.
    ///
    /// # Errors
    ///
    /// Fails when a length is negative, not finite or too long for a buffer.
    pub fn buffer_size(self, density: Density) -> Result<(u32, u32), NumberError> {
        let pixels = |length: Dp| {
            let pixels = NonNegative::try_new(density.to_px(length).get().ceil())?;
            pixels.get().to_u32().ok_or(NumberError::OutOfRange)
        };
        Ok((pixels(self.width)?, pixels(self.height)?))
    }
}

/// A width and a height in whole physical pixels, such as the size of a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// The width and the height as the signed 32-bit lengths of native APIs, or None for a size
    /// that does not fit them.
    #[must_use]
    pub fn signed(self) -> Option<(i32, i32)> {
        Some((
            i32::try_from(self.width).ok()?,
            i32::try_from(self.height).ok()?,
        ))
    }
}

/// The area laid out for a view or for a part of it, together with the density of the surface
/// showing it. `Vw` and `Vh` lengths are percentages of `size`: 100 vw is its width and 100 vh is its height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub size: Size,
    pub density: Density,
}

impl Viewport {
    #[must_use]
    pub const fn new(size: Size, density: Density) -> Self {
        Self { size, density }
    }

    /// The length of one physical pixel at the density of the viewport.
    #[must_use]
    pub fn pixel(self) -> Dp {
        self.density.to_dp(Px::new(1.0))
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(Size::ZERO, Density::ONE)
    }
}

/// The viewport passed to `View::frame`, or the viewport provided by a wrapper for the part of
/// the view it wraps.
pub static VIEWPORT: Local<Viewport> = Local::new(Viewport::default);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    pub x: Dp,
    pub y: Dp,
    pub width: Dp,
    pub height: Dp,
}

impl Rect {
    #[must_use]
    pub const fn from_size(size: Size) -> Self {
        Self::new(Dp::ZERO, Dp::ZERO, size.width, size.height)
    }

    #[must_use]
    pub const fn new(x: Dp, y: Dp, width: Dp, height: Dp) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    #[must_use]
    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn bounding(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        (points.into_iter())
            .map(|p| Self::new(p.x, p.y, Dp::ZERO, Dp::ZERO))
            .reduce(Self::union)
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            (self.x + self.width).max(other.x + other.width) - x,
            (self.y + self.height).max(other.y + other.height) - y,
        )
    }

    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let width = (self.x + self.width).min(other.x + other.width) - x;
        let height = (self.y + self.height).min(other.y + other.height) - y;
        (width > Dp::ZERO && height > Dp::ZERO).then_some(Self::new(x, y, width, height))
    }

    /// Moves every side outward by `amount`. A negative amount moves the sides inward until the rectangle is empty.
    #[must_use]
    pub fn outset(self, amount: Dp) -> Self {
        Self::new(
            self.x - amount,
            self.y - amount,
            (self.width + amount * 2.0).max(Dp::ZERO),
            (self.height + amount * 2.0).max(Dp::ZERO),
        )
    }

    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        Self::new(
            self.x + offset.x,
            self.y + offset.y,
            self.width,
            self.height,
        )
    }

    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }

    #[must_use]
    pub fn inset(self, i: Insets) -> Self {
        Self::new(
            self.x + i.left,
            self.y + i.top,
            (self.width - i.horizontal()).max(Dp::ZERO),
            (self.height - i.vertical()).max(Dp::ZERO),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Insets {
    pub top: Dp,
    pub right: Dp,
    pub bottom: Dp,
    pub left: Dp,
}

impl Insets {
    #[must_use]
    pub const fn all(value: Dp) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    #[must_use]
    pub const fn xy(horizontal: Dp, vertical: Dp) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    #[must_use]
    pub fn horizontal(self) -> Dp {
        self.left + self.right
    }

    #[must_use]
    pub fn vertical(self) -> Dp {
        self.top + self.bottom
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Scale {
    pub x: f64,
    pub y: f64,
}

/// A length in dp, or in percent of the viewport or of a box. `to_dp` converts it into dp once the size of the
/// viewport and of the box are known.
/// A bare number is not a length:
/// ```compile_fail
/// # use lavashell_core::geometry::Length;
/// let offset: Length = 10.0.into();
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Dp(Dp),
    Vw(Vw),
    Vh(Vh),
    Pct(Pct),
}

impl Length {
    /// This length in dp, where `extent` is the length of the box along the axis it measures.
    #[must_use]
    pub fn to_dp(self, extent: Dp, viewport: Size) -> Dp {
        match self {
            Self::Dp(length) => length,
            Self::Vw(percent) => viewport.width * percent.get() / 100.0,
            Self::Vh(percent) => viewport.height * percent.get() / 100.0,
            Self::Pct(percent) => extent * percent.get() / 100.0,
        }
    }
}

impl Default for Length {
    fn default() -> Self {
        Self::Dp(Dp::ZERO)
    }
}

macro_rules! length {
    ($($unit:ident),*) => {$(
        impl From<$unit> for Length {
            fn from(length: $unit) -> Self {
                Self::$unit(length)
            }
        }
    )*};
}

length!(Dp, Vw, Vh, Pct);

mod axis;
mod constraints;

pub use crate::color::Color;
use crate::local::Local;
use crate::output::finest_step;
use crate::units::{Density, Dp, FiniteDp, NonNegative, NumberError, Pct, Px, Vh, Vw};
pub use axis::Axis;
pub use constraints::{Constraints, Span};
use num_traits::ToPrimitive;

#[cfg(test)]
mod tests;
