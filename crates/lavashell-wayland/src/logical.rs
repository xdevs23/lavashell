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

/// The nearest whole logical pixels of the width and the height of `size`, for protocol requests
/// that take integers, or `None` when a length is not finite or does not fit an `i32`.
pub(crate) fn protocol_size(size: Size) -> Option<(i32, i32)> {
    Some((whole(size.width)?, whole(size.height)?))
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

/// The nearest whole logical pixels of a window size, or `None` when [`whole_size`] fails or a length rounds to zero.
pub(crate) fn window_size(size: Size) -> Option<(u32, u32)> {
    whole_size(size).filter(|&(width, height)| width > 0 && height > 0)
}
