// SPDX-License-Identifier: GPL-3.0-only

use lavashell_core::geometry::{Point, Size};
use lavashell_core::units::Dp;
use num_traits::ToPrimitive;

const MINIMUM_SURFACE_EXTENT: u32 = 1;

pub(crate) fn convert_to_point(x: f64, y: f64) -> Point {
    Point::new(Dp::new(x), Dp::new(y))
}

pub(crate) fn convert_to_size((width, height): (u32, u32)) -> Size {
    Size::new(Dp::new(f64::from(width)), Dp::new(f64::from(height)))
}

pub(crate) fn round_to_i32(length: Dp) -> Option<i32> {
    length.round().get().to_i32()
}

pub(crate) fn round_to_u32(length: Dp) -> Option<u32> {
    u32::try_from(round_to_i32(length)?).ok()
}

pub(crate) fn to_protocol_surface_size(size: Size) -> Option<(i32, i32)> {
    let extent = |length: Dp| i32::try_from(round_to_u32(length)?.max(MINIMUM_SURFACE_EXTENT)).ok();
    Some((extent(size.width)?, extent(size.height)?))
}

pub(crate) fn to_protocol_position(point: Point) -> Option<(i32, i32)> {
    round_to_i32(point.x).zip(round_to_i32(point.y))
}

pub(crate) fn to_protocol_size(size: Size) -> Option<(u32, u32)> {
    round_to_u32(size.width).zip(round_to_u32(size.height))
}

#[cfg(test)]
mod tests;
