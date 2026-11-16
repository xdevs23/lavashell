// SPDX-License-Identifier: GPL-3.0-only
use super::{Glyphs, TextAntialiasing, TextDestination, TextRendering};
use crate::Error;
use crate::raster::{
    ParagraphRaster, ParagraphTile, RasterCache, TileBudget, TileContent, Uncached,
    resource_cache_limit,
};
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;
use skia_safe::{
    AlphaType, Canvas, Color, Color4f, ColorType, Image, ImageInfo, Point, Rect, SurfaceProps,
};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;

pub(crate) struct RasterInput<'a> {
    pub canvas: &'a Canvas,
    pub policy: TextRendering,
    pub destination: TextDestination,
    pub opaque_scope: bool,
    pub style: &'a TextStyle<Dp>,
    pub foreground: Color4f,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
    Painted,
    NotApplicable,
}

pub(crate) trait TextRasterizer {
    fn antialiasing(&self) -> TextAntialiasing;

    fn source_style(&self, input: &RasterInput<'_>) -> Option<TextStyle<Dp>>;

    fn source_rendering(&self, input: &RasterInput<'_>) -> TextRendering {
        input.policy
    }

    fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: Point,
        source: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error>;
}

#[derive(Clone, Copy)]
pub(super) enum CoverageBackground {
    Black,
    Transparent,
}

impl CoverageBackground {
    fn color(self) -> Color {
        match self {
            Self::Black => Color::BLACK,
            Self::Transparent => Color::TRANSPARENT,
        }
    }

    fn alpha_type(self) -> AlphaType {
        match self {
            Self::Black => AlphaType::Opaque,
            Self::Transparent => AlphaType::Premul,
        }
    }
}

pub(super) struct CoverageTile {
    tile: ParagraphTile,
    content: TileContent,
}

impl CoverageTile {
    pub fn new(
        input: &RasterInput<'_>,
        source: &mut ShapedParagraph,
        origin: Point,
        scale: f32,
        key: u64,
        raster: ParagraphRaster,
        images_sharing_cache: usize,
    ) -> Option<Self> {
        let budget = TileBudget {
            bytes_per_pixel: ColorType::BGRA8888.bytes_per_pixel(),
            max_bytes: resource_cache_limit(input.canvas) / images_sharing_cache,
        };
        let tile =
            ParagraphTile::at_integer_scale(input.canvas, source.ink(), origin, scale, budget)?;
        let content = TileContent::paragraph(key, raster, scale, tile.phase);
        Some(Self { tile, content })
    }

    pub fn destination(&self) -> Rect {
        self.tile.destination
    }

    pub fn image(
        &self,
        input: &RasterInput<'_>,
        tiles: &mut RasterCache,
        source: &mut ShapedParagraph,
        background: CoverageBackground,
        props: Option<&SurfaceProps>,
    ) -> Result<Image, Error> {
        let info = ImageInfo::new(
            self.tile.size,
            ColorType::BGRA8888,
            background.alpha_type(),
            None,
        );
        let (scale, origin) = (self.tile.scale, self.tile.paint_origin);
        let content = self.content.clone();
        tiles
            .cached_image(
                input.canvas,
                content,
                self.tile.bytes,
                &info,
                props,
                |canvas| {
                    let _chained_canvas = canvas.clear(background.color()).scale((scale, scale));
                    source.paragraph.paint(canvas, origin);
                },
            )
            .map_err(|uncached| match uncached {
                Uncached::Unallocated => Error::SubpixelTile,
                Uncached::Empty | Uncached::TooLarge => Error::SubpixelTileSize,
            })
    }
}

pub(crate) struct Registration {
    pub create: fn() -> Result<Box<dyn TextRasterizer>, Error>,
}

inventory::collect!(Registration);

pub(crate) struct Rasterizers(HashMap<TextAntialiasing, Rc<dyn TextRasterizer>>);

impl Rasterizers {
    pub fn new() -> Result<Self, Error> {
        let mut rasterizers = HashMap::new();
        for registration in inventory::iter::<Registration> {
            let rasterizer: Rc<dyn TextRasterizer> = Rc::from((registration.create)()?);
            match rasterizers.entry(rasterizer.antialiasing()) {
                Entry::Occupied(taken) => {
                    return Err(Error::DuplicateTextRasterizer(*taken.key()));
                }
                Entry::Vacant(slot) => {
                    let _registered_rasterizer = slot.insert(rasterizer);
                }
            }
        }
        Ok(Self(rasterizers))
    }

    pub fn reads_destination(&self, policy: TextRendering) -> bool {
        self.0.contains_key(&policy.antialiasing)
    }

    pub fn rasterization(&self, input: &RasterInput<'_>) -> Option<Rasterization> {
        let rasterizer = self.0.get(&input.policy.antialiasing)?;
        Some(Rasterization {
            style: rasterizer.source_style(input)?,
            rendering: rasterizer.source_rendering(input),
            rasterizer: Rc::clone(rasterizer),
        })
    }
}

pub(crate) struct Rasterization {
    pub style: TextStyle<Dp>,
    pub rendering: TextRendering,
    rasterizer: Rc<dyn TextRasterizer>,
}

impl Rasterization {
    pub fn draw(
        &self,
        input: &RasterInput<'_>,
        origin: Point,
        paragraph: &mut ShapedParagraph,
        key: u64,
        tiles: &mut RasterCache,
    ) -> Result<Drawn, Error> {
        if paragraph.glyphs() != Glyphs::Coverage {
            return Ok(Drawn::NotApplicable);
        }
        self.rasterizer.draw(input, origin, paragraph, key, tiles)
    }
}
