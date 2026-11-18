// SPDX-License-Identifier: GPL-3.0-only
use super::key::{ParagraphRaster, TileContent};
use super::tiles::{TileSpec, draw_image};
use super::{ParagraphTile, RasterCache, RasterMode, TileBudget, resource_cache_limit};
use crate::failure::DrawFailure;
use crate::text_cache::ShapedParagraph;
use skia_safe::{Canvas, Image, Point, Rect};

impl RasterCache {
    pub fn paragraph(
        &mut self,
        canvas: &Canvas,
        paragraph: &mut ShapedParagraph,
        key: Option<u64>,
        origin: Point,
    ) {
        let tile = match self.mode {
            RasterMode::Tiles => self.paragraph_image(canvas, paragraph, key, origin),
            RasterMode::Vector => Ok(None),
        };
        match tile.inspect_err(|failure| self.failures.record(*failure)) {
            Ok(Some((image, destination))) => draw_image(canvas, &image, destination),
            Ok(None) | Err(_) => paragraph.paragraph.paint(canvas, origin),
        }
    }

    pub(crate) fn paragraph_image(
        &mut self,
        canvas: &Canvas,
        paragraph: &mut ShapedParagraph,
        key: Option<u64>,
        origin: Point,
    ) -> Result<Option<(Image, Rect)>, DrawFailure> {
        let budget = TileBudget {
            bytes_per_pixel: canvas.image_info().bytes_per_pixel(),
            max_bytes: resource_cache_limit(canvas),
        };
        let Some(tile_geometry) = ParagraphTile::new(canvas, paragraph.ink(), origin, budget)
        else {
            return Ok(None);
        };
        let tile_geometry = match key {
            Some(_) => Some(tile_geometry),
            None => (canvas.local_clip_bounds())
                .and_then(|clip| tile_geometry.clipped(clip, budget.bytes_per_pixel)),
        };
        let Some(tile_geometry) = tile_geometry else {
            return Ok(None);
        };
        let spec = TileSpec {
            retained_as: key.map(|key| {
                TileContent::paragraph(
                    key,
                    ParagraphRaster::Native,
                    tile_geometry.scale,
                    tile_geometry.phase,
                )
            }),
            bytes: tile_geometry.bytes,
            size: tile_geometry.size,
            destination: tile_geometry.destination,
        };
        let image = self.tile_image(canvas, &spec, |tile| {
            let _canvas_returned_for_chaining =
                tile.scale((tile_geometry.scale, tile_geometry.scale));
            paragraph.paragraph.paint(tile, tile_geometry.paint_origin);
        })?;
        Ok(Some((image, spec.destination)))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_error::TestError;
    use crate::test_font::PROBE_FAMILY;
    use crate::test_surfaces::Pixels;
    use lavashell_core::prelude::*;

    #[test]
    fn tiles_match_direct_paragraphs_at_integer_and_fractional_origins() -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?;
        for size in [11.0, 13.0, 16.0] {
            for phase in [0.0, 0.25, 0.5, 0.75] {
                let render =
                    |mode: crate::raster::RasterMode| -> Result<Pixels<[u8; 4]>, TestError> {
                        let mut painter =
                            crate::Painter::without_device(&fonts)?.with_raster_mode(mode);
                        painter.set_blend_space(crate::BlendSpace::Srgb);
                        let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
                            ui.text(
                                "MON 5 Oct Lavashell 18:42",
                                Modifier::new()
                                    .font_family(PROBE_FAMILY)
                                    .font_size(Sp::new(size))
                                    .color(Color::WHITE)
                                    .offset(Dp::new(12.0 + phase), Dp::new(8.0 + phase)),
                            );
                        });
                        let viewport = Viewport::new(Size::new(240.dp(), 40.dp()), Density::ONE);
                        let commands =
                            crate::test_scenes::framed_commands(&mut view, viewport, &mut painter)?;
                        let info = crate::test_surfaces::srgb_info((240, 40));
                        let mut surface = skia_safe::surfaces::raster(&info, None, None)
                            .ok_or(TestError::RasterSurface)?;
                        let _canvas_returned_for_chaining =
                            surface.canvas().clear(skia_safe::Color::BLACK);
                        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
                        Ok(crate::test_surfaces::pixels(&mut surface))
                    };
                let direct = render(crate::raster::RasterMode::Vector)?;
                let tiled = render(crate::raster::RasterMode::Tiles)?;
                let largest = direct.largest_channel_difference(&tiled)?;
                assert!(direct.iter().any(|p| p[0] > 0), "empty font rendering");
                assert!(
                    largest <= 1,
                    "size={size}, phase={phase}, max={largest}; a tile stores each pixel once \
                     more than direct drawing, which rounds a channel by at most one step"
                );
            }
        }
        Ok(())
    }
}
