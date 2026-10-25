// SPDX-License-Identifier: GPL-3.0-only
use super::{Glyphs, TextDestination, TextRendering};
use crate::Error;
use crate::raster::{RasterCache, Uncached};
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, Color4f, Image};

pub(crate) struct RasterInput<'a> {
    pub canvas: &'a Canvas,
    pub policy: TextRendering,
    pub destination: TextDestination,
    pub opaque_scope: bool,
    pub style: &'a TextStyle<Dp>,
    /// The color of `style` as linear sRGB floats, the form in which the rasterizers pass a color to Skia.
    pub foreground: Color4f,
}

/// What a rasterizer did with a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
    Painted,
    /// The rasterizer does not apply to the paragraph or to its destination. The ordinary paragraph rendering paints
    /// the paragraph instead.
    NotApplicable,
}

pub(crate) trait TextRasterizer {
    fn reads_destination(&self, policy: TextRendering) -> bool;

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>>;

    fn source_rendering(&self, input: &RasterInput<'_>) -> TextRendering {
        input.policy
    }

    fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: (f32, f32),
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error>;
}

/// The image of a coverage tile returned by the raster cache. `ParagraphTile` already sized the tile to fit the cache.
/// A tile reported by the cache as empty or too large therefore becomes `Error::SubpixelTileSize`, and a tile that the
/// cache could not allocate becomes `Error::SubpixelTile`.
pub(super) fn coverage_tile(image: Result<Image, Uncached>) -> Result<Image, Error> {
    image.map_err(|uncached| match uncached {
        Uncached::Unallocated => Error::SubpixelTile,
        Uncached::Empty | Uncached::TooLarge => Error::SubpixelTileSize,
    })
}

/// The registration of one text rasterizer. Each rasterizer applies to text rendering policies to which no other
/// rasterizer applies. The order in which the registrations are collected therefore does not change which rasterizer
/// draws a paragraph.
pub(crate) struct Registration {
    pub create: fn() -> Result<Box<dyn TextRasterizer>, Error>,
}

inventory::collect!(Registration);

pub(crate) struct Rasterizers(Vec<Box<dyn TextRasterizer>>);

impl Rasterizers {
    /// # Errors
    /// Fails when a rasterizer cannot compile its effects.
    pub fn new() -> Result<Self, Error> {
        (inventory::iter::<Registration>.into_iter())
            .map(|entry| (entry.create)())
            .collect::<Result<_, _>>()
            .map(Self)
    }

    pub fn reads_destination(&self, policy: TextRendering) -> bool {
        self.0
            .iter()
            .any(|provider| provider.reads_destination(policy))
    }

    pub fn source(&self, input: &RasterInput<'_>) -> Option<(usize, TextStyle<Dp>, TextRendering)> {
        self.0.iter().enumerate().find_map(|(index, provider)| {
            provider
                .source_style(input)
                .map(|style| (index, style, provider.source_rendering(input)))
        })
    }

    /// Draws `source` with the rasterizer at `index`. A paragraph with color or bitmap glyphs returns
    /// `Drawn::NotApplicable` and is painted by the ordinary paragraph rendering, because those glyphs contain colors
    /// and no coverage.
    pub fn draw(
        &self,
        index: usize,
        input: &RasterInput<'_>,
        origin: (f32, f32),
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error> {
        if source.glyphs() != Glyphs::Coverage {
            return Ok(Drawn::NotApplicable);
        }
        self.0[index].draw(input, origin, source, key, tiles)
    }
}
