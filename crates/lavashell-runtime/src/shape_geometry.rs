// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::Dp;
use lavashell_renderer::skia::{IRect, Path};
use lavashell_renderer::{InputSizeError, OutlineError, input_cells, outline_path};

/// The input cells and the clip path of the outline of a window shape, computed by the renderer.
pub(crate) struct ShapeGeometry {
    /// The whole logical pixels inside the outline, in dp.
    pub input_cells: Vec<Rect>,
    /// The path in dp that clips what the surface paints.
    pub clip: Path,
}

/// A failure to compute the geometry of the outline of a window shape.
#[derive(Debug, thiserror::Error)]
pub enum ShapeGeometryError {
    #[error("the path of a window shape could not be built")]
    Path(#[source] OutlineError),
    #[error("the input region of a window shape could not be computed")]
    Input(#[source] InputSizeError),
}

impl ShapeGeometry {
    /// The geometry of `outline` on a surface of `size`.
    ///
    /// # Errors
    ///
    /// Fails when the path of the outline cannot be built, or when its input region cannot be computed.
    pub fn new(outline: &SurfaceOutline, size: Size) -> Result<Self, ShapeGeometryError> {
        let clip = outline_path(outline).map_err(ShapeGeometryError::Path)?;
        let cells = input_cells(&clip, size).map_err(ShapeGeometryError::Input)?;
        Ok(Self {
            input_cells: cells.iter().map(cell_rect).collect(),
            clip,
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
