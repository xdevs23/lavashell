// SPDX-License-Identifier: GPL-3.0-only
//! RGBA tiles, rasterized once and reused, that replace the path masks and the tessellated glyph paths of Skia on
//! drivers with broken alpha-mask coverage. Other devices draw paths directly. Path tiles and paragraph tiles share
//! one cache. The cache contains the tiles that a surface drew in its current or previous frame. A tile is drawn with
//! the transform and the clip of the canvas. A path clip uses the coverage of the tile pixels.

mod geometry;
mod key;
mod paragraph;
mod storage;

pub(crate) use geometry::{ParagraphTile, ink_bounds, pixel_bytes};
pub(crate) use key::{ParagraphRaster, TileContent};
pub(crate) use tiles::Uncached;

mod path;
#[cfg(test)]
mod tests;
mod tiles;
mod upload;

use crate::retention::{Frames, Retained};
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, Image};

/// The width in device pixels that antialiasing covers outside the drawn content.
const ANTIALIASING_PIXELS: f32 = 1.0;

/// The most device pixels by which a tile moves when its start is aligned to a whole pixel.
const ALIGNMENT_PIXELS: f32 = 1.0;

/// The device pixels around the drawn content that a tile includes.
const EDGE_PIXELS: f32 = ANTIALIASING_PIXELS + ALIGNMENT_PIXELS;

struct Tile {
    image: Image,
    bytes: usize,
}

pub(crate) struct RasterCache {
    raster: bool,
    without_storage: bool,
    uploads: upload::UploadBatch,
    tiles: Retained<key::TileKey, Tile>,
    frames: Frames,
}

impl RasterCache {
    pub fn new(raster: bool) -> Self {
        Self {
            raster,
            without_storage: false,
            uploads: upload::UploadBatch::default(),
            tiles: Retained::default(),
            frames: Frames::default(),
        }
    }

    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.frames.begin(surface);
        self.tiles.retain_recent(&self.frames);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.frames.forget(surface);
    }

    /// Drops every tile. The frame counts of the surfaces are unchanged.
    pub fn clear(&mut self) {
        self.tiles = Retained::default();
    }
}

impl Default for RasterCache {
    fn default() -> Self {
        Self::new(false)
    }
}

/// The resource cache limit in bytes of the GPU context behind `canvas`. When `canvas` has no GPU context, the byte
/// limit of the Skia cache for the images of raster canvases.
pub(crate) fn resource_cache_limit(canvas: &Canvas) -> usize {
    canvas.direct_context().map_or_else(
        skia_safe::graphics::resource_cache_total_bytes_limit,
        |context| context.resource_cache_limit(),
    )
}
