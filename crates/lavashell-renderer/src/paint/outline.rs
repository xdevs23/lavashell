// SPDX-License-Identifier: GPL-3.0-only
//! The path of a surface outline, which clips what the surface paints.

use super::{rounded, skpath};
use crate::failure::DrawFailure;
use crate::scalar::scalar;
use lavashell_core::shape::{Polygon, RoundedRect};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::{Density, Dp};
use skia_safe::{Path, PathBuilder, PathFillType};

#[derive(Debug, thiserror::Error)]
pub enum OutlineError {
    #[error("the rounded surface outline {0:?} is not finite")]
    Rounded(RoundedRect),
    #[error("a number of a surface outline lies beyond the range of Skia scalars")]
    Scalar(#[source] DrawFailure),
    #[error("the surface outline lies beyond the range of Skia scalars once scaled by {0:?}")]
    Scaled(Density),
}

/// The path of `outline` in dp.
///
/// # Errors
///
/// Fails when a rounded outline is not finite, or when a number lies beyond the range of Skia scalars.
pub fn outline_path(outline: &SurfaceOutline) -> Result<Path, OutlineError> {
    match outline {
        SurfaceOutline::Empty => Ok(Path::new()),
        SurfaceOutline::Contours(contours) => contour_path(contours),
        SurfaceOutline::Rounded(shape) => rounded_path(shape),
    }
}

/// `outline`, a path in dp, scaled to physical pixels at `density`.
///
/// # Errors
///
/// Fails when `density` or the scaled path lies beyond the range of Skia scalars.
pub fn clip_path(outline: &Path, density: Density) -> Result<Path, OutlineError> {
    let scale = scalar(density.get()).map_err(OutlineError::Scalar)?;
    outline
        .try_make_scale((scale, scale))
        .ok_or(OutlineError::Scaled(density))
}

/// The even-odd path through all `contours`.
fn contour_path(contours: &[Polygon]) -> Result<Path, OutlineError> {
    let mut path = PathBuilder::new();
    let _chained_builder = path.set_fill_type(PathFillType::EvenOdd);
    for contour in contours {
        let contour = skpath(contour).map_err(OutlineError::Scalar)?;
        let _chained_builder = path.add_path(&contour, None);
    }
    Ok(path.detach())
}

/// The path around `shape`, which is empty when the width or the height of `shape` is not positive.
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
