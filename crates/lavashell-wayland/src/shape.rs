// SPDX-License-Identifier: GPL-3.0-only
//! The outline of a window shape in surface-local dp. The same outline clips what the surface paints and sets the
//! input region sent to the compositor.

mod rounded;

use crate::{Error, logical, single_precision};
use lavashell_core::geometry::{Point, Size};
use lavashell_core::units::{Density, Dp};
use lavashell_renderer::skia::{self, IRect, Path, PathBuilder, PathFillType, Region, region};
use std::rc::Rc;

type Contours = dyn Fn(Size) -> Vec<Vec<Point>>;

#[derive(Clone)]
pub struct WindowShape {
    outline: Outline,
    pub(crate) clip_paint: bool,
    pub(crate) accept_input: bool,
}

#[derive(Clone)]
enum Outline {
    Contours(Rc<Contours>),
    RoundedRect(Rc<rounded::RoundedRect>),
}

impl WindowShape {
    /// A shape made of the closed contours returned by `outline` for the size of the surface, with points in dp.
    /// The contours are filled by the even-odd rule, so a contour inside another one cuts a hole. `outline` runs
    /// again when the size of the surface or the reactive state read by it changes. Every contour needs at least
    /// three finite points.
    pub fn new(outline: impl Fn(Size) -> Vec<Vec<Point>> + 'static) -> Self {
        Self::from_outline(Outline::Contours(Rc::new(outline)))
    }

    fn from_outline(outline: Outline) -> Self {
        Self {
            outline,
            clip_paint: true,
            accept_input: true,
        }
    }

    /// Uses the outline only for the input region. The surface also paints outside the outline.
    #[must_use]
    pub fn input_only(mut self) -> Self {
        self.clip_paint = false;
        self
    }

    /// Uses the outline to clip painting and the background effects. The input region sent to the compositor is
    /// empty.
    #[must_use]
    pub fn input_passthrough(mut self) -> Self {
        self.accept_input = false;
        self
    }

    pub(crate) fn compile(&self, size: Size, density: Density) -> Result<ShapeGeometry, Error> {
        let cells = covered_cells(size).ok_or(Error::ShapeSize(size))?;
        let outline = match &self.outline {
            Outline::Contours(contours) => contour_path(&contours(size))?,
            Outline::RoundedRect(rect) => rounded::path(rect(size))?,
        };
        let mut input = Region::new();
        // Wayland regions consist of whole logical pixels. Skia puts a pixel into the region when its center is
        // inside the outline.
        let input_cells = if input.set_path(&outline, &Region::from_rect(cells)) {
            region::Iterator::new(&input).collect()
        } else {
            Vec::new()
        };
        let scale = scalar(density.get())?;
        let path = outline.make_scale((scale, scale));
        if !path.is_finite() {
            return Err(Error::ShapeContour);
        }
        Ok(ShapeGeometry { path, input_cells })
    }
}

pub(crate) struct ShapeGeometry {
    /// The outline in physical pixels.
    pub path: Path,
    /// Rectangles of whole logical pixels inside the outline.
    pub input_cells: Vec<IRect>,
}

/// The whole logical pixels that cover `size`, or `None` when `size` is empty, not finite or longer than the
/// protocol allows.
fn covered_cells(size: Size) -> Option<IRect> {
    let cells = |length: Dp| logical::whole(length.ceil()).filter(|&cells| cells > 0);
    Some(IRect::from_wh(cells(size.width)?, cells(size.height)?))
}

/// The Skia scalar nearest to `value`.
fn scalar(value: f64) -> Result<f32, Error> {
    single_precision::nearest(value).ok_or(Error::ShapeScalar(value))
}

/// The even-odd path in dp through the closed `contours`.
fn contour_path(contours: &[Vec<Point>]) -> Result<Path, Error> {
    let corner = |point: &Point| -> Result<skia::Point, Error> {
        Ok(skia::Point::new(
            scalar(point.x.get())?,
            scalar(point.y.get())?,
        ))
    };
    let corners: Vec<Vec<skia::Point>> = contours
        .iter()
        .map(|contour| contour.iter().map(corner).collect())
        .collect::<Result<_, _>>()?;
    let valid = |contour: &Vec<skia::Point>| {
        contour.len() >= 3 && contour.iter().all(|corner| corner.is_finite())
    };
    if !corners.iter().all(valid) {
        return Err(Error::ShapeContour);
    }
    let mut path = PathBuilder::new();
    Ok(corners
        .iter()
        .fold(
            path.set_fill_type(PathFillType::EvenOdd),
            |path, contour| path.add_polygon(contour, true),
        )
        .detach())
}

#[cfg(test)]
mod tests;
