// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_renderer::test_surfaces::{TestSurface, image_pixels};
use lavashell_renderer::{Fonts, RasterMode};

const SURFACE: TestSurface = TestSurface::new(400, 240);

#[test]
fn thousands_of_tiny_tiles_render_correctly_across_upload_batches() -> Result<(), TestError> {
    let (columns, rows) = (100_u8, 60_u8);
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(SURFACE.size(), Density::ONE));
    let mut drawing = Drawing::new(&mut commands, SURFACE.bounds(), &context);
    for y in 0..rows {
        for x in 0..columns {
            let (left, top) = (4.dp() * f64::from(x), 4.dp() * f64::from(y));
            drawing.path(
                &[
                    Point::new(left, top),
                    Point::new(left + 4.dp(), top),
                    Point::new(left + 4.dp(), top + 4.dp()),
                    Point::new(left, top + 4.dp()),
                ],
                true,
                Color::rgb(if (x + y) % 2 == 0 {
                    0x00ff_0000
                } else {
                    0x0000_00ff
                }),
                None,
            );
        }
    }
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu
        .painter(&Fonts::new())?
        .with_raster_mode(RasterMode::Tiles);
    let mut surface = gpu.offscreen(SURFACE.pixel_size())?;
    let limit = surface.image_info().compute_min_byte_size();
    surface
        .canvas()
        .direct_context()
        .ok_or(TestError::Missing("GPU context of the canvas"))?
        .set_resource_cache_limit(limit);
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
    let png = gpu.png(&mut surface)?;
    let uploads = painter.pending_tile_uploads();
    assert!(uploads.bytes <= limit);
    assert!(uploads.images < usize::from(columns) * usize::from(rows));
    let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&png))
        .ok_or(lavashell_renderer::Error::DecodeImage)?;
    let pixels = image_pixels(&image, skia_safe::AlphaType::Unpremul);
    for y in 0..u16::from(rows) {
        for x in 0..u16::from(columns) {
            let pixel = pixels.at(x * 4 + 2, y * 4 + 2);
            let dominant = if (x + y) % 2 == 0 { 0 } else { 2 };
            assert!(
                pixel[dominant] == 255 && pixel[2 - dominant] == 0,
                "wrong tile at{x},{y}: {pixel:?}"
            );
        }
    }
    Ok(())
}
