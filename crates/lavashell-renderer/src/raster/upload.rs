// SPDX-License-Identifier: GPL-3.0-only

use skia_safe::{Canvas, gpu::PurgeResourceOptions};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct UploadBatch {
    images: HashSet<u64>,
    bytes: usize,
}

impl UploadBatch {
    fn would_exceed(&self, id: u64, bytes: usize, limit: usize) -> bool {
        !self.images.contains(&id)
            && !self.images.is_empty()
            && self.bytes.saturating_add(bytes) > limit
    }

    fn record(&mut self, id: u64, bytes: usize) {
        if self.images.insert(id) {
            self.bytes = self.bytes.saturating_add(bytes);
        }
    }

    pub fn before_draw(&mut self, canvas: &Canvas, id: u64, bytes: usize) {
        let Some(mut context) = canvas.direct_context() else {
            return;
        };
        if self.would_exceed(id, bytes, context.resource_cache_limit()) {
            let _context_returned_for_chaining = context.flush_submit_and_sync_cpu();
            let _context_returned_for_chaining =
                context.purge_unlocked_resources(PurgeResourceOptions::AllResources);
            self.images.clear();
            self.bytes = 0;
        }
        self.record(id, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_pending_uploads_hit_the_byte_limit_but_reuse_does_not() {
        let limit = 64 * 1024;
        for bytes in [4, 16 * 1024] {
            let mut pending = UploadBatch::default();
            let mut batches = 0;
            for id in (0_u64..).take(2 * limit / bytes) {
                if pending.would_exceed(id, bytes, limit) {
                    assert!(pending.bytes <= limit);
                    batches += 1;
                    pending = UploadBatch::default();
                }
                pending.record(id, bytes);
                assert!(!pending.would_exceed(id, bytes, limit));
                pending.record(id, bytes);
                assert_eq!(pending.bytes, pending.images.len() * bytes);
            }
            assert!(batches > 0);
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use crate::test_error::TestError;
    use crate::test_surfaces::{TestSurface, image_pixels};
    use lavashell_core::prelude::*;

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
        let mut gpu = crate::VulkanRenderer::headless()?;
        let mut painter = gpu
            .painter(&crate::Fonts::new())?
            .with_raster_mode(crate::raster::RasterMode::Tiles);
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
        let tiles = painter.resources.tiles.borrow();
        assert!(tiles.uploads.bytes <= limit);
        assert!(tiles.uploads.images.len() < usize::from(columns) * usize::from(rows));
        let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&png))
            .ok_or(crate::Error::DecodeImage)?;
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
}
