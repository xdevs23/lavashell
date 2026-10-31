// SPDX-License-Identifier: GPL-3.0-only
use super::key::{ParagraphRaster, TileContent};
use super::{ParagraphTile, RasterCache, resource_cache_limit};
use crate::text_cache::ShapedParagraph;
use skia_safe::{Canvas, Image, Paint, Rect};

impl RasterCache {
    pub fn paragraph(
        &mut self,
        canvas: &Canvas,
        paragraph: &mut ShapedParagraph,
        key: u64,
        origin: (f32, f32),
    ) {
        if !self.raster {
            paragraph.paragraph.paint(canvas, origin);
            return;
        }
        let Some((image, destination)) = self.paragraph_image(canvas, paragraph, key, origin)
        else {
            paragraph.paragraph.paint(canvas, origin);
            return;
        };
        let _chained_canvas = canvas.draw_image_rect_with_sampling_options(
            &image,
            None,
            destination,
            skia_safe::SamplingOptions::new(
                skia_safe::FilterMode::Linear,
                skia_safe::MipmapMode::None,
            ),
            &Paint::default(),
        );
    }

    /// The tile of the paragraph and its destination rectangle on `canvas`. Skia paints the paragraph into a
    /// premultiplied tile, which applies the gamma correction of Skia to each glyph before the glyphs overlap.
    pub(crate) fn paragraph_image(
        &mut self,
        canvas: &Canvas,
        paragraph: &mut ShapedParagraph,
        key: u64,
        origin: (f32, f32),
    ) -> Option<(Image, Rect)> {
        let bytes_per_pixel = canvas.image_info().bytes_per_pixel();
        let tile_geometry = ParagraphTile::new(
            canvas,
            paragraph.ink(),
            origin,
            bytes_per_pixel,
            resource_cache_limit(canvas),
        )?;
        let tile_geometry = if self.without_storage {
            tile_geometry.clipped(canvas.local_clip_bounds()?, bytes_per_pixel)?
        } else {
            tile_geometry
        };
        let content = TileContent::paragraph(
            key,
            ParagraphRaster::Native,
            tile_geometry.scale,
            tile_geometry.phase,
        );
        let info = canvas
            .image_info()
            .with_dimensions(tile_geometry.size)
            .with_alpha_type(skia_safe::AlphaType::Premul);
        let image = self
            .cached_image(canvas, content, tile_geometry.bytes, &info, None, |tile| {
                let _chained_canvas = tile.clear(skia_safe::Color::TRANSPARENT);
                let _chained_canvas = tile.scale((tile_geometry.scale, tile_geometry.scale));
                paragraph.paragraph.paint(tile, tile_geometry.paint_origin);
            })
            .ok()?;
        Some((image, tile_geometry.destination))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_error::TestError;
    use lavashell_core::prelude::*;
    use lavashell_test_platform::WithoutFailures;

    #[test]
    fn tiles_match_direct_paragraphs_at_integer_and_fractional_origins() -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?;
        for size in [11.0, 13.0, 16.0] {
            for phase in [0.0, 0.25, 0.5, 0.75] {
                let render = |fallback: bool| -> Result<Vec<u8>, TestError> {
                    let mut painter =
                        crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?
                            .with_raster_paths(fallback);
                    painter.set_blend_space(crate::BlendSpace::Srgb);
                    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
                        ui.text(
                            "MON 5 Oct Lavashell 18:42",
                            Modifier::new()
                                .font_family("probe")
                                .font_size(Sp::new(size))
                                .color(Color::WHITE)
                                .offset(Dp::new(12.0 + phase), Dp::new(8.0 + phase)),
                        );
                    });
                    let frame = view.frame(
                        Viewport::new(Size::new(240.dp(), 40.dp()), Density::ONE),
                        &mut painter.shaper,
                    );
                    let commands = crate::test_scenes::leaf_commands(frame.without_failures()?)?;
                    let info = skia_safe::ImageInfo::new(
                        (240, 40),
                        skia_safe::ColorType::RGBA8888,
                        skia_safe::AlphaType::Premul,
                        skia_safe::ColorSpace::new_srgb(),
                    );
                    let mut surface = skia_safe::surfaces::raster(&info, None, None)
                        .ok_or(TestError::RasterSurface)?;
                    let _chained_canvas = surface.canvas().clear(skia_safe::Color::BLACK);
                    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
                    let mut pixels = vec![0_u8; 240 * 40 * 4];
                    assert!(surface.read_pixels(&info, &mut pixels, 240 * 4, (0, 0)));
                    Ok(pixels)
                };
                let direct = render(false)?;
                let tiled = render(true)?;
                let differences: Vec<_> = direct
                    .iter()
                    .zip(&tiled)
                    .map(|(a, b)| a.abs_diff(*b))
                    .collect();
                assert!(
                    direct.as_chunks::<4>().0.iter().any(|p| p[0] > 0),
                    "empty font rendering"
                );
                assert!(
                    differences.iter().all(|d| *d <= 1),
                    "size={size}, phase={phase}, max={:?}",
                    differences.iter().max()
                );
            }
        }
        Ok(())
    }
}
