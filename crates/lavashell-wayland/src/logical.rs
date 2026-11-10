// SPDX-License-Identifier: GPL-3.0-only
//! Conversions between Wayland surface-local coordinates and dp. Surface-local coordinates are in logical pixels.
//! One logical pixel is one dp.

use lavashell_core::geometry::{Point, Size};
use lavashell_core::units::Dp;
use num_traits::ToPrimitive;

pub(crate) fn length(logical: f64) -> Dp {
    Dp::new(logical)
}

pub(crate) fn point(x: f64, y: f64) -> Point {
    Point::new(length(x), length(y))
}

pub(crate) fn size((width, height): (u32, u32)) -> Size {
    Size::new(length(f64::from(width)), length(f64::from(height)))
}

/// The nearest whole logical pixel, for protocol requests that take integers, or `None` when
/// the length is not finite or does not fit an `i32`.
pub(crate) fn whole(length: Dp) -> Option<i32> {
    length.round().get().to_i32()
}

/// The whole logical pixels of a surface of `size`, each length rounded and at least one pixel, or `None` when a
/// length is negative, not finite or does not fit an `i32`.
pub(crate) fn surface_size(size: Size) -> Option<(i32, i32)> {
    let length = |length: Dp| i32::try_from(extent(length, 1)?).ok();
    Some((length(size.width)?, length(size.height)?))
}

/// The nearest whole logical pixel of a size, at least `minimum`, or `None` when the length is
/// negative, not finite or does not fit an `i32`.
pub(crate) fn extent(length: Dp, minimum: u32) -> Option<u32> {
    let whole = u32::try_from(whole(length)?).ok()?;
    Some(whole.max(minimum))
}

/// The nearest whole logical pixels of a size, or `None` when a length is negative, not finite or does not fit an
/// `i32`.
pub(crate) fn whole_size(size: Size) -> Option<(u32, u32)> {
    extent(size.width, 0).zip(extent(size.height, 0))
}

#[cfg(test)]
mod tests;
