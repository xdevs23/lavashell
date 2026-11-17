// SPDX-License-Identifier: GPL-3.0-only

use super::{rounded, skpath};
use crate::failure::DrawFailure;
use lavashell_core::shape::{Polygon, RoundedRect};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::Dp;
use skia_safe::{Path, PathBuilder, PathFillType};

#[derive(Debug, thiserror::Error)]
pub enum OutlineError {
    #[error("the rounded surface outline {0:?} is not finite")]
    Rounded(RoundedRect),
    #[error("a number of a surface outline lies beyond the range of Skia scalars")]
    Scalar(#[source] DrawFailure),
}

pub fn outline_path(outline: &SurfaceOutline) -> Result<Path, OutlineError> {
    match outline {
        SurfaceOutline::Empty => Ok(Path::new()),
        SurfaceOutline::Contours(contours) => contour_path(contours),
        SurfaceOutline::Rounded(shape) => rounded_path(shape),
    }
}

fn contour_path(contours: &[Polygon]) -> Result<Path, OutlineError> {
    let mut path = PathBuilder::new();
    let _builder_returned_for_chaining = path.set_fill_type(PathFillType::EvenOdd);
    for contour in contours {
        let contour = skpath(contour).map_err(OutlineError::Scalar)?;
        let _builder_returned_for_chaining = path.add_path(&contour, None);
    }
    Ok(path.detach())
}

fn rounded_path(shape: &RoundedRect) -> Result<Path, OutlineError> {
    let rect = shape.rect;
    let lengths = [rect.x, rect.y, rect.width, rect.height];
    if !(lengths.iter().chain(&shape.corners.to_array())).all(|length| length.is_finite()) {
        return Err(OutlineError::Rounded(*shape));
    }
    if rect.width <= Dp::ZERO || rect.height <= Dp::ZERO {
        return Ok(Path::new());
    }
    let corners = rounded(shape).map_err(OutlineError::Scalar)?;
    Ok(PathBuilder::new().add_rrect(corners, None, None).detach())
}

#[cfg(test)]
mod tests;
