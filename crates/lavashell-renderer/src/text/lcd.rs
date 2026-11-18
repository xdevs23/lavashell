// SPDX-License-Identifier: GPL-3.0-only
use super::TextAntialiasing;
use super::raster::{
    Coverage, CoverageBackground, CoverageTile, RasterInput, Registration, TextRasterizer,
    sampled_mask,
};
use crate::Error;
use crate::raster::{ParagraphRaster, ParagraphTile, RasterCache};
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;

inventory::submit! { Registration { create: || Ok(Box::new(OpaqueCoverage)) } }

struct OpaqueCoverage;

fn scale(input: &RasterInput<'_>) -> Option<f32> {
    if !(input.policy).subpixel_applies(input.destination, input.opaque_scope) {
        return None;
    }
    ParagraphTile::integer_scale(input.canvas)
}

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

    fn coverage(
        &self,
        input: &RasterInput<'_>,
        origin: skia_safe::Point,
        source: &mut ShapedParagraph,
        key: Option<u64>,
        tiles: &mut RasterCache,
    ) -> Result<Option<Coverage>, Error> {
        let Some(scale) = scale(input) else {
            return Ok(None);
        };
        let raster = ParagraphRaster::OpaqueCoverage;
        let Some(tile) = CoverageTile::new(input, source, origin, scale, key, raster, 1) else {
            return Ok(None);
        };
        let props = input.destination.surface_props();
        let image = tile.image(
            input,
            tiles,
            source,
            CoverageBackground::Black,
            Some(&props),
        )?;
        Ok(Some(Coverage {
            shader: sampled_mask(&image, tile.destination())?,
            destination: tile.destination(),
        }))
    }

    fn draw(&self, input: &RasterInput<'_>, coverage: &Coverage) -> Result<(), Error> {
        (input.rgb_blend).draw(input.canvas, coverage, input.foreground)
    }
}
