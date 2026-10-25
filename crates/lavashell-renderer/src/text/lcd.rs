// SPDX-License-Identifier: GPL-3.0-only
use super::coverage::RgbBlend;
use super::raster::{Drawn, RasterInput, Registration, TextRasterizer, coverage_tile};
use super::{TextAntialiasing, TextRendering};
use crate::Error;
use crate::raster::{
    ParagraphRaster, ParagraphTile, RasterCache, TileContent, resource_cache_limit,
};
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorType, ImageInfo};

inventory::submit! { Registration { create } }

fn create() -> Result<Box<dyn TextRasterizer>, Error> {
    Ok(Box::new(OpaqueCoverage {
        blend: RgbBlend::new()?,
    }))
}

struct OpaqueCoverage {
    blend: RgbBlend,
}

fn scale(input: &RasterInput<'_>) -> Option<f32> {
    if !input.opaque_scope || !input.destination.opaque {
        return None;
    }
    ParagraphTile::integer_scale(input.canvas)
}

#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod tests;

impl TextRasterizer for OpaqueCoverage {
    fn reads_destination(&self, policy: TextRendering) -> bool {
        policy.antialiasing == TextAntialiasing::Subpixel
    }

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>> {
        if !self.reads_destination(input.policy) || scale(input).is_none() {
            return None;
        }
        let mut style = input.style.clone();
        style.color = Color::WHITE;
        Some(style)
    }

    fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: (f32, f32),
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error> {
        let Some(scale) = scale(input) else {
            return Ok(Drawn::NotApplicable);
        };
        let Some(tile) = ParagraphTile::at_integer_scale(
            input.canvas,
            source.ink(),
            origin,
            scale,
            ColorType::BGRA8888.bytes_per_pixel(),
            resource_cache_limit(input.canvas),
        ) else {
            return Ok(Drawn::NotApplicable);
        };
        let info = ImageInfo::new(tile.size, ColorType::BGRA8888, AlphaType::Opaque, None);
        let props = input.destination.surface_props();
        let content =
            TileContent::paragraph(key, ParagraphRaster::OpaqueCoverage, scale, tile.phase);
        let image = tiles.cached_image(
            input.canvas,
            content,
            tile.bytes,
            &info,
            Some(&props),
            |canvas| {
                let _chained_canvas = canvas.clear(skia_safe::Color::BLACK).scale((scale, scale));
                source.paragraph.paint(canvas, tile.paint_origin);
            },
        );
        let image = coverage_tile(image)?;
        (self.blend).draw(input.canvas, &image, tile.destination, input.foreground)?;
        Ok(Drawn::Painted)
    }
}
