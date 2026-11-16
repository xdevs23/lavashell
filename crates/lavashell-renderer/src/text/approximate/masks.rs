// SPDX-License-Identifier: GPL-3.0-only
use super::super::TextDestination;
use super::super::raster::{CoverageBackground, CoverageTile, RasterInput};
use crate::Error;
use crate::raster::{ParagraphRaster, ParagraphTile, RasterCache};
use crate::text_cache::ShapedParagraph;
use skia_safe::{Image, Point, Rect};

const MASK_IMAGES: usize = 2;

pub(super) fn scale(input: &RasterInput<'_>) -> Option<f32> {
    if !(input.policy).subpixel_applies(input.destination, input.opaque_scope)
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
    pub fn paint(
        input: &RasterInput<'_>,
        origin: Point,
        source: &mut ShapedParagraph,
        key: Option<u64>,
        tiles: &mut RasterCache,
    ) -> Result<Option<Self>, Error> {
        let Some(scale) = scale(input) else {
            return Ok(None);
        };
        let raster = ParagraphRaster::TranslucentCoverage;
        let Some(tile) = CoverageTile::new(input, source, origin, scale, key, raster, MASK_IMAGES)
        else {
            return Ok(None);
        };
        let props = TextDestination {
            opaque: true,
            geometry: input.destination.geometry,
        }
        .surface_props();
        Ok(Some(Self {
            rgb: tile.image(
                input,
                tiles,
                source,
                CoverageBackground::Black,
                Some(&props),
            )?,
            center: tile.image(input, tiles, source, CoverageBackground::Transparent, None)?,
            destination: tile.destination(),
        }))
    }
}
