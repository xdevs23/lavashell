// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::{Density, Dp};
use lavashell_renderer::skia::{IRect, Path};
use lavashell_renderer::{InputSizeError, OutlineError, clip_path, input_cells, outline_path};

pub(crate) struct ShapeGeometry {
    pub input_cells: Vec<Rect>,
    pub clip: Path,
}

#[derive(Debug, thiserror::Error)]
pub enum ShapeGeometryError {
    #[error("the path of a window shape could not be built")]
    Path(#[source] OutlineError),
    #[error("the input region of a window shape could not be computed")]
    Input(#[source] InputSizeError),
}

impl ShapeGeometry {
    pub fn new(
        outline: &SurfaceOutline,
        size: Size,
        density: Density,
    ) -> Result<Self, ShapeGeometryError> {
        let path = outline_path(outline).map_err(ShapeGeometryError::Path)?;
        let cells = input_cells(&path, size).map_err(ShapeGeometryError::Input)?;
        Ok(Self {
            input_cells: cells.iter().map(cell_rect).collect(),
            clip: clip_path(&path, density).map_err(ShapeGeometryError::Path)?,
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
