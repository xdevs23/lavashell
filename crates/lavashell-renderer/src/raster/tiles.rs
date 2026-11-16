// SPDX-License-Identifier: GPL-3.0-only
use super::key::{TileContent, TileKey};
use super::{RasterCache, Tile, resource_cache_limit};
use crate::failure::DrawFailure;
use skia_safe::{Canvas, Color, ISize, Image, ImageInfo, Paint, Rect, SurfaceProps};

pub(super) fn sampling() -> skia_safe::SamplingOptions {
    skia_safe::SamplingOptions::new(skia_safe::FilterMode::Linear, skia_safe::MipmapMode::None)
}

pub(super) fn draw_image(canvas: &Canvas, image: &Image, destination: Rect) {
    let _canvas_returned_for_chaining = canvas.draw_image_rect_with_sampling_options(
        image,
        None,
        destination,
        sampling(),
        &Paint::default(),
    );
}

pub(super) struct TileSpec {
    pub retained_as: Option<TileContent>,
    pub bytes: usize,
    pub size: ISize,
    pub destination: Rect,
}

#[derive(Clone, Copy)]
pub(crate) enum Uncached {
    Empty,
    TooLarge,
    Unallocated,
}

impl From<Uncached> for DrawFailure {
    fn from(uncached: Uncached) -> Self {
        match uncached {
            Uncached::Empty | Uncached::TooLarge => Self::TileSize,
            Uncached::Unallocated => Self::Allocation,
        }
    }
}

impl RasterCache {
    pub(super) fn draw_tile(
        &mut self,
        canvas: &Canvas,
        tile: &TileSpec,
        rasterize: impl FnOnce(&Canvas),
    ) -> Result<(), DrawFailure> {
        let image = self.tile_image(canvas, tile, rasterize)?;
        draw_image(canvas, &image, tile.destination);
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
        let retained_as = tile.retained_as.clone();
        self.cached_image(canvas, retained_as, tile.bytes, &info, None, |target| {
            let _canvas_returned_for_chaining = target.clear(Color::TRANSPARENT);
            rasterize(target);
        })
        .map_err(DrawFailure::from)
    }

    pub(crate) fn cached_image(
        &mut self,
        canvas: &Canvas,
        retained_as: Option<TileContent>,
        bytes: usize,
        info: &ImageInfo,
        props: Option<&SurfaceProps>,
        rasterize: impl FnOnce(&Canvas),
    ) -> Result<Image, Uncached> {
        if info.width() <= 0 || info.height() <= 0 {
            return Err(Uncached::Empty);
        }
        let rasterized = || -> Result<Tile, Uncached> {
            let mut tile =
                skia_safe::surfaces::raster(info, None, props).ok_or(Uncached::Unallocated)?;
            rasterize(tile.canvas());
            Ok(Tile {
                image: tile.image_snapshot(),
                bytes,
            })
        };
        let tile = match retained_as {
            None => rasterized()?,
            Some(content) => {
                if bytes > resource_cache_limit(canvas) {
                    return Err(Uncached::TooLarge);
                }
                let key = TileKey::new(content, info, props);
                (self
                    .tiles
                    .get_or_try_make(key, &self.frames, |_| rasterized()))?
                .clone()
            }
        };
        self.uploads
            .before_draw(canvas, u64::from(tile.image.unique_id()), tile.bytes);
        Ok(tile.image)
    }
}
