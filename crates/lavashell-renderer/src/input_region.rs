// SPDX-License-Identifier: GPL-3.0-only

use lavashell_core::geometry::Size;
use lavashell_core::units::Dp;
use num_traits::ToPrimitive;
use skia_safe::{IRect, Path, Region, region};

#[derive(Debug, thiserror::Error)]
#[error("input cells need a finite positive surface size, not {0:?}")]
pub struct InputSizeError(pub Size);

pub fn input_cells(outline: &Path, size: Size) -> Result<Vec<IRect>, InputSizeError> {
    let cells = covered_cells(size).ok_or(InputSizeError(size))?;
    let mut input = Region::new();
    Ok(if input.set_path(outline, &Region::from_rect(cells)) {
        region::Iterator::new(&input).collect()
    } else {
        Vec::new()
    })
}

fn covered_cells(size: Size) -> Option<IRect> {
    let cells = |length: Dp| length.ceil().get().to_i32().filter(|&cells| cells > 0);
    Some(IRect::from_wh(cells(size.width)?, cells(size.height)?))
}
