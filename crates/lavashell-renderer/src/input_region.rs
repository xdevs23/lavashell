// SPDX-License-Identifier: GPL-3.0-only
//! The whole logical pixels in which a surface receives input.

use lavashell_core::geometry::Size;
use lavashell_core::units::Dp;
use num_traits::ToPrimitive;
use skia_safe::{IRect, Path, Region, region};

#[derive(Debug, thiserror::Error)]
#[error("input cells need a finite positive surface size, not {0:?}")]
pub struct InputSizeError(pub Size);

/// Rectangles of the whole logical pixels of a surface of `size` whose center lies inside `outline`, a path in dp.
///
/// # Errors
///
/// Fails when a length of `size` is not finite and positive, or lies beyond the range of Skia rectangles.
pub fn input_cells(outline: &Path, size: Size) -> Result<Vec<IRect>, InputSizeError> {
    let cells = covered_cells(size).ok_or(InputSizeError(size))?;
    let mut input = Region::new();
    Ok(if input.set_path(outline, &Region::from_rect(cells)) {
        region::Iterator::new(&input).collect()
    } else {
        Vec::new()
    })
}

/// The whole logical pixels that cover `size`, or `None` when `size` is empty, not finite or longer than a Skia
/// rectangle.
fn covered_cells(size: Size) -> Option<IRect> {
    let cells = |length: Dp| length.ceil().get().to_i32().filter(|&cells| cells > 0);
    Some(IRect::from_wh(cells(size.width)?, cells(size.height)?))
}
