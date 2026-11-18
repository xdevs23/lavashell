// SPDX-License-Identifier: GPL-3.0-only

mod geometry;
mod key;
mod paragraph;
mod path;
#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-support"))]
impl RasterCache {
    pub(crate) fn images(&self) -> impl Iterator<Item = &Image> {
        self.tiles.values().map(|tile| &tile.image)
    }

    pub(crate) fn pending_uploads(&self) -> crate::test_readings::TileUploads {
        self.uploads.pending()
    }
}
mod tiles;
mod upload;

use crate::failure::Failures;
use crate::retention::{Frames, Retained};
pub(crate) use geometry::{
    ParagraphTile, TileBudget, ink_bounds, outset_by_antialiasing, pixel_bytes,
};
pub(crate) use key::{ParagraphRaster, TileContent};
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, Image};
pub(crate) use tiles::Uncached;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterMode {
    Vector,
    Tiles,
}

const ANTIALIASING_PIXELS: f32 = 1.0;

const ALIGNMENT_PIXELS: f32 = 1.0;

const EDGE_PIXELS: f32 = ANTIALIASING_PIXELS + ALIGNMENT_PIXELS;

#[derive(Clone)]
struct Tile {
    image: Image,
    bytes: usize,
}

pub(crate) struct RasterCache {
    mode: RasterMode,
    uploads: upload::UploadBatch,
    tiles: Retained<key::TileKey, Tile>,
    frames: Frames,
    failures: Failures,
}

impl RasterCache {
    pub fn new(mode: RasterMode, failures: Failures) -> Self {
        Self {
            mode,
            uploads: upload::UploadBatch::default(),
            tiles: Retained::default(),
            frames: Frames::default(),
            failures,
        }
    }

    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.frames.begin(surface);
        self.tiles.retain_recent(&self.frames);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.frames.forget(surface);
    }

    pub fn clear(&mut self) {
        self.tiles = Retained::default();
    }
}

pub(crate) fn resource_cache_limit(canvas: &Canvas) -> usize {
    canvas.direct_context().map_or_else(
        skia_safe::graphics::resource_cache_total_bytes_limit,
        |context| context.resource_cache_limit(),
    )
}
