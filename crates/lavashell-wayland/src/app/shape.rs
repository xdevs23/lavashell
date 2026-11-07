// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::{Density, Dp};
use lavashell_renderer::skia::{IRect, Path};
use lavashell_renderer::{clip_path, input_cells, outline_path};

/// The input cells and the clip path of the outline of a window shape, computed by the renderer.
pub(super) struct ShapeGeometry {
    /// The whole logical pixels inside the outline, in dp.
    pub input_cells: Vec<Rect>,
    /// The path in physical pixels that clips what the surface paints.
    pub clip: Path,
}

impl ShapeGeometry {
    /// The geometry of `outline` on a surface of `size` at `density`.
    pub fn new(outline: &SurfaceOutline, size: Size, density: Density) -> Result<Self, Error> {
        let path = outline_path(outline).map_err(Error::ShapePath)?;
        let cells = input_cells(&path, size).map_err(Error::ShapeInput)?;
        Ok(Self {
            input_cells: cells.iter().map(cell_rect).collect(),
            clip: clip_path(&path, density).map_err(Error::ShapePath)?,
        })
    }
}

fn cell_rect(cell: &IRect) -> Rect {
    let length = |pixels: i32| Dp::new(f64::from(pixels));
    Rect::new(
        length(cell.left),
        length(cell.top),
        length(cell.width()),
        length(cell.height()),
    )
}
