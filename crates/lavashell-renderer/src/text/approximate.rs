// SPDX-License-Identifier: GPL-3.0-only
use super::raster::{Drawn, RasterInput, Registration, TextRasterizer};
use super::{TextAntialiasing, TextRendering};
use crate::Error;
use crate::raster::RasterCache;
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;
use skia_safe::Color4f;

mod blend;
#[cfg(test)]
mod color_tests;
#[cfg(test)]
mod gpu_tests;
mod masks;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod samples;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;

inventory::submit! { Registration { create } }

fn create() -> Result<Box<dyn TextRasterizer>, Error> {
    Ok(Box::new(ApproximateCoverage::new()?))
}

struct ApproximateCoverage {
    blend: blend::MaskBlend,
}

impl ApproximateCoverage {
    fn new() -> Result<Self, Error> {
        Ok(Self {
            blend: blend::MaskBlend::new()?,
        })
    }
}

impl TextRasterizer for ApproximateCoverage {
    fn reads_destination(&self, policy: TextRendering) -> bool {
        policy.antialiasing == TextAntialiasing::SubpixelApproximate
    }

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>> {
        if !self.reads_destination(input.policy)
            || masks::scale(input).is_none()
            || !input.style.color.in_gamut(Gamut::Srgb)
        {
            return None;
        }
        let mut style = input.style.clone();
        // The mask is drawn with the alpha of the text color, so Skia applies that alpha to each glyph before
        // overlapping glyphs are combined. An opaque mask with the alpha applied afterwards would draw the overlap of
        // two glyphs as lightly as a single glyph.
        style.color = Color::WHITE.with_alpha(input.style.color.alpha());
        Some(style)
    }

    fn source_rendering(&self, input: &RasterInput<'_>) -> TextRendering {
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..input.policy
        }
    }

    fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: (f32, f32),
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error> {
        let Some(masks) = masks::Masks::paint(input, origin, source, key, tiles)? else {
            return Ok(Drawn::NotApplicable);
        };
        let opaque = Color4f {
            a: 1.0,
            ..input.foreground
        };
        (self.blend).draw(input.canvas, &masks, opaque)?;
        Ok(Drawn::Painted)
    }
}
