// SPDX-License-Identifier: GPL-3.0-only
use super::TextAntialiasing;
use super::coverage::RgbBlend;
use super::raster::{
    CoverageBackground, CoverageTile, Drawn, RasterInput, Registration, TextRasterizer,
};
use crate::Error;
use crate::raster::{ParagraphRaster, ParagraphTile, RasterCache};
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;

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
    fn antialiasing(&self) -> TextAntialiasing {
        TextAntialiasing::Subpixel
    }

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>> {
        scale(input).map(|_| TextStyle {
            color: Color::WHITE,
            ..input.style.clone()
        })
    }

    fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: skia_safe::Point,
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error> {
        let Some(scale) = scale(input) else {
            return Ok(Drawn::NotApplicable);
        };
        let raster = ParagraphRaster::OpaqueCoverage;
        let Some(tile) = CoverageTile::new(input, source, origin, scale, key, raster, 1) else {
            return Ok(Drawn::NotApplicable);
        };
        let props = input.destination.surface_props();
        let image = tile.image(
            input,
            tiles,
            source,
            CoverageBackground::Black,
            Some(&props),
        )?;
        (self.blend).draw(input.canvas, &image, tile.destination(), input.foreground)?;
        Ok(Drawn::Painted)
    }
}
