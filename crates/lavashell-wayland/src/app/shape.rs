// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::geometry::Size;
use lavashell_core::reactive::{Phase, Scope};
use lavashell_core::units::Density;
use lavashell_renderer::skia::{IRect, Path};
use lavashell_renderer::{clip_path, input_cells, outline_path};
use lavashell_runtime::WindowShape;
use sctk::compositor::{CompositorState, Region};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit as sctk;

pub(super) struct SurfaceShape {
    shape: WindowShape,
    key: Option<(Size, Density)>,
    clip: Option<Path>,
    input_cells: Vec<IRect>,
    scope: Scope,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShapeUpdate {
    Unchanged,
    Changed,
}

impl SurfaceShape {
    pub fn new(shape: WindowShape) -> Self {
        Self {
            shape,
            key: None,
            clip: None,
            input_cells: Vec::new(),
            scope: Scope::default(),
        }
    }

    pub fn update(
        &mut self,
        size: Size,
        density: Density,
        surface: &WlSurface,
        compositor: &CompositorState,
    ) -> Result<ShapeUpdate, Error> {
        if self.key == Some((size, density)) && !self.needs_frame() {
            return Ok(ShapeUpdate::Unchanged);
        }
        let outline = self.scope.track(Phase::Layout, || self.shape.outline(size));
        let path =
            outline_path(&outline.map_err(Error::ShapeContour)?).map_err(Error::ShapePath)?;
        let cells = input_cells(&path, size).map_err(Error::ShapeInput)?;
        self.clip = Some(clip_path(&path, density).map_err(Error::ShapePath)?);
        self.input_cells = cells;
        self.key = Some((size, density));
        let region = if self.shape.accepts_input() {
            self.region(compositor)?
        } else {
            Region::new(compositor).map_err(Error::CreateRegion)?
        };
        surface.set_input_region(Some(region.wl_region()));
        surface.set_opaque_region(None);
        Ok(ShapeUpdate::Changed)
    }

    pub fn needs_frame(&self) -> bool {
        self.scope.dirty().any()
    }

    pub fn accepts_input(&self) -> bool {
        self.shape.accepts_input() && !self.input_cells.is_empty()
    }

    pub fn path(&self) -> Option<&Path> {
        self.clip.as_ref().filter(|_| self.shape.clips_paint())
    }

    /// A Wayland region made of the input cells inside the shape. The region is empty until `update` first
    /// computes the input cells of the shape.
    pub fn region(&self, compositor: &CompositorState) -> Result<Region, Error> {
        let region = Region::new(compositor).map_err(Error::CreateRegion)?;
        for cells in &self.input_cells {
            region.add(cells.left, cells.top, cells.width(), cells.height());
        }
        Ok(region)
    }
}
