// SPDX-License-Identifier: GPL-3.0-only
use super::raster::{Coverage, RasterInput, Registration, TextRasterizer};
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
    fn antialiasing(&self) -> TextAntialiasing {
        TextAntialiasing::SubpixelApproximate
    }

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>> {
        if masks::scale(input).is_none() || !input.style.color.in_gamut(Gamut::Srgb) {
            return None;
        }
        let mut style = input.style.clone();
        style.color = Color::WHITE.with_alpha(input.style.color.alpha());
        Some(style)
    }

    fn source_rendering(&self, input: &RasterInput<'_>) -> TextRendering {
        TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..input.policy
        }
    }

    fn coverage(
        &self,
        input: &RasterInput<'_>,
        origin: skia_safe::Point,
        source: &mut ShapedParagraph,
        key: Option<u64>,
        tiles: &mut RasterCache,
    ) -> Result<Option<Coverage>, Error> {
        masks::Masks::paint(input, origin, source, key, tiles)?
            .map(|masks| self.blend.coverage(&masks))
            .transpose()
    }

    fn draw(&self, input: &RasterInput<'_>, coverage: &Coverage) -> Result<(), Error> {
        let opaque = Color4f {
            a: 1.0,
            ..input.foreground
        };
        (self.blend).draw(input.canvas, coverage, opaque)
    }
}
