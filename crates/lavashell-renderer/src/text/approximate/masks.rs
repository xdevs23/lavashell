// SPDX-License-Identifier: GPL-3.0-only
use super::super::raster::{RasterInput, coverage_tile};
use super::super::{PixelGeometry, TextDestination};
use crate::Error;
use crate::raster::{
    ParagraphRaster, ParagraphTile, RasterCache, TileContent, resource_cache_limit,
};
use crate::text_cache::ShapedParagraph;
use skia_safe::{AlphaType, ColorType, Image, ImageInfo, Rect};

/// The number of mask images that one paragraph puts into the Skia resource cache: the RGB mask and the center mask.
/// Each of them may use at most this fraction of the cache limit.
const MASK_IMAGES: usize = 2;

/// The integer scale at which the masks of a paragraph are drawn. Returns `None` outside an opaque scope, on a
/// destination without a subpixel geometry, for a text size of zero or less, and on a canvas whose transform is not a
/// whole-pixel scale. Large text is not excluded: Skia draws it with grayscale coverage, which the mask blend draws
/// like any other coverage.
pub(super) fn scale(input: &RasterInput<'_>) -> Option<f32> {
    if !input.opaque_scope
        || input.destination.geometry == PixelGeometry::None
        || input.style.size.get() <= 0.0
    {
        return None;
    }
    ParagraphTile::integer_scale(input.canvas)
}

pub(super) struct Masks {
    pub rgb: Image,
    pub center: Image,
    pub destination: Rect,
}

impl Masks {
    /// Draws the RGB mask and the center mask of `source` through the raster cache. Returns `None` where [`scale`]
    /// returns `None`, and where the tile of the paragraph is empty, larger than the canvas allows or larger than
    /// its share of the resource cache.
    pub fn paint(
        input: &RasterInput<'_>,
        origin: (f32, f32),
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Option<Self>, Error> {
        let Some(scale) = scale(input) else {
            return Ok(None);
        };
        let Some(tile) = ParagraphTile::at_integer_scale(
            input.canvas,
            source.ink(),
            origin,
            scale,
            ColorType::BGRA8888.bytes_per_pixel(),
            resource_cache_limit(input.canvas) / MASK_IMAGES,
        ) else {
            return Ok(None);
        };
        let content =
            TileContent::paragraph(key, ParagraphRaster::TranslucentCoverage, scale, tile.phase);
        // The mask raster is declared opaque, because the surface properties set the subpixel geometry only for an
        // opaque destination. The destination declared by the application is still translucent, and the alpha returned
        // by composition is unchanged.
        let props = TextDestination {
            opaque: true,
            geometry: input.destination.geometry,
        }
        .surface_props();
        let rgb_info = ImageInfo::new(tile.size, ColorType::BGRA8888, AlphaType::Opaque, None);
        let rgb = tiles.cached_image(
            input.canvas,
            content.clone(),
            tile.bytes,
            &rgb_info,
            Some(&props),
            |canvas| {
                let _chained_canvas = canvas.clear(skia_safe::Color::BLACK).scale((scale, scale));
                source.paragraph.paint(canvas, tile.paint_origin);
            },
        );
        let rgb = coverage_tile(rgb)?;
        let center_info = rgb_info.with_alpha_type(AlphaType::Premul);
        let center = tiles.cached_image(
            input.canvas,
            content,
            tile.bytes,
            &center_info,
            None,
            |canvas| {
                let _chained_canvas = canvas
                    .clear(skia_safe::Color::TRANSPARENT)
                    .scale((scale, scale));
                source.paragraph.paint(canvas, tile.paint_origin);
            },
        );
        Ok(Some(Self {
            rgb,
            center: coverage_tile(center)?,
            destination: tile.destination,
        }))
    }
}
