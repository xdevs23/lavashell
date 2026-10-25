// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::shape::{ShapeGeometry, WindowShape};
use lavashell_core::geometry::Size;
use lavashell_core::reactive::{Phase, Scope};
use lavashell_core::units::Density;
use sctk::compositor::{CompositorState, Region};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit as sctk;

pub(super) struct SurfaceShape {
    outline: WindowShape,
    key: Option<(Size, Density)>,
    geometry: Option<ShapeGeometry>,
    scope: Scope,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShapeUpdate {
    Unchanged,
    Changed,
}

impl SurfaceShape {
    pub fn new(outline: WindowShape) -> Self {
        Self {
            outline,
            key: None,
            geometry: None,
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
        let geometry = self
            .scope
            .track(Phase::Layout, || self.outline.compile(size, density))?;
        self.geometry = Some(geometry);
        self.key = Some((size, density));
        let region = if self.outline.accept_input {
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
        self.outline.accept_input
            && self
                .geometry
                .as_ref()
                .is_some_and(|geometry| !geometry.input_cells.is_empty())
    }

    pub fn path(&self) -> Option<&lavashell_renderer::skia::Path> {
        self.outline
            .clip_paint
            .then(|| self.geometry.as_ref().map(|geometry| &geometry.path))
            .flatten()
    }

    /// A Wayland region made of the input cells inside the shape. The region is empty until `update` first
    /// computes the geometry of the shape.
    pub fn region(&self, compositor: &CompositorState) -> Result<Region, Error> {
        let region = Region::new(compositor).map_err(Error::CreateRegion)?;
        for cells in self
            .geometry
            .iter()
            .flat_map(|geometry| &geometry.input_cells)
        {
            region.add(cells.left, cells.top, cells.width(), cells.height());
        }
        Ok(region)
    }
}
