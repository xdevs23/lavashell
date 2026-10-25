// SPDX-License-Identifier: GPL-3.0-only
use super::key::{TileContent, TileKey};
use super::{RasterCache, Tile, resource_cache_limit};
use crate::failure::DrawFailure;
use skia_safe::{Canvas, Color, ISize, Image, ImageInfo, Paint, Rect, SurfaceProps};

/// Linear sampling without mipmaps. A tile has the device resolution, but a transform can place it between device
/// pixels.
pub(super) fn sampling() -> skia_safe::SamplingOptions {
    skia_safe::SamplingOptions::new(skia_safe::FilterMode::Linear, skia_safe::MipmapMode::None)
}

pub(super) struct TileSpec {
    pub content: TileContent,
    pub bytes: usize,
    pub size: ISize,
    pub destination: Rect,
}

/// Why the raster cache has no image of a tile.
pub(crate) enum Uncached {
    /// The image has no pixels.
    Empty,
    /// The image has more bytes than the resource cache limit of the canvas.
    TooLarge,
    /// No raster surface could be allocated for the image.
    Unallocated,
}

impl RasterCache {
    pub(super) fn draw_tile(
        &mut self,
        canvas: &Canvas,
        tile: &TileSpec,
        rasterize: impl FnOnce(&Canvas),
    ) -> Result<(), DrawFailure> {
        let image = self.tile_image(canvas, tile, rasterize)?;
        let _chained_canvas = canvas.draw_image_rect_with_sampling_options(
            &image,
            None,
            tile.destination,
            sampling(),
            &Paint::default(),
        );
        Ok(())
    }

    pub(super) fn tile_image(
        &mut self,
        canvas: &Canvas,
        tile: &TileSpec,
        rasterize: impl FnOnce(&Canvas),
    ) -> Result<Image, DrawFailure> {
        let info = canvas
            .image_info()
            .with_dimensions(tile.size)
            .with_alpha_type(skia_safe::AlphaType::Premul);
        let content = tile.content.clone();
        self.cached_image(canvas, content, tile.bytes, &info, None, |target| {
            let _chained_canvas = target.clear(Color::TRANSPARENT);
            rasterize(target);
        })
        .map_err(|uncached| match uncached {
            Uncached::Empty | Uncached::TooLarge => DrawFailure::TileSize,
            Uncached::Unallocated => DrawFailure::Allocation,
        })
    }

    /// The image of `content`, rasterized by `rasterize` when the cache has none.
    ///
    /// # Errors
    /// Fails when the image is empty, larger than the resource cache of `canvas`, or no raster surface can be
    /// allocated for it.
    pub(crate) fn cached_image(
        &mut self,
        canvas: &Canvas,
        content: TileContent,
        bytes: usize,
        info: &ImageInfo,
        props: Option<&SurfaceProps>,
        rasterize: impl FnOnce(&Canvas),
    ) -> Result<Image, Uncached> {
        if info.width() <= 0 || info.height() <= 0 {
            return Err(Uncached::Empty);
        }
        if self.without_storage {
            let mut tile =
                skia_safe::surfaces::raster(info, None, props).ok_or(Uncached::Unallocated)?;
            rasterize(tile.canvas());
            let image = tile.image_snapshot();
            self.uploads
                .before_draw(canvas, u64::from(image.unique_id()), bytes);
            return Ok(image);
        }
        if bytes > resource_cache_limit(canvas) {
            return Err(Uncached::TooLarge);
        }
        let key = TileKey::new(content, info, props);
        let made = self.tiles.get_or_try_make(key, &self.frames, |_| {
            let mut tile =
                skia_safe::surfaces::raster(info, None, props).ok_or(Uncached::Unallocated)?;
            rasterize(tile.canvas());
            Ok(Tile {
                image: tile.image_snapshot(),
                bytes,
            })
        })?;
        let image = made.image.clone();
        self.uploads
            .before_draw(canvas, u64::from(image.unique_id()), made.bytes);
        Ok(image)
    }
}
