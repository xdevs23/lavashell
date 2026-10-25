// SPDX-License-Identifier: GPL-3.0-only
//! Limits the bytes of the tile images uploaded since the last flush to the resource cache limit of the GPU context.
//! The limit counts only these pending uploads, not the tiles stored in the raster cache.

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
            // A flush alone can leave every upload in flight. Waiting for the GPU
            // to finish this batch lets Skia release the textures that no draw locks.
            // The purge leaves the targets of active layers alone, and the caller
            // still presents the frame afterwards.
            let _chained_context = context.flush_submit_and_sync_cpu();
            let _chained_context =
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
            for id in 0..(2 * limit / bytes) as u64 {
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
    use lavashell_core::prelude::*;

    #[test]
    fn thousands_of_tiny_tiles_render_correctly_across_upload_batches() -> Result<(), TestError> {
        let (columns, rows) = (100_u8, 60_u8);
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(Size::new(400.dp(), 240.dp()), Density::ONE));
        let mut drawing = Drawing::new(
            &mut commands,
            Rect::new(0.dp(), 0.dp(), 400.dp(), 240.dp()),
            &context,
        );
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
        let mut painter = gpu.painter(&crate::Fonts::new())?.with_raster_paths(true);
        let mut surface = gpu.offscreen((400, 240))?;
        // The tiles together have several times the pixels of the surface. A resource cache limit of the byte size
        // of the surface therefore splits their uploads into several batches.
        let limit = surface.image_info().compute_min_byte_size();
        surface
            .canvas()
            .direct_context()
            .ok_or(TestError::Missing("GPU context of the canvas"))?
            .set_resource_cache_limit(limit);
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut surface)?;
        let tiles = painter.resources.tiles.borrow();
        assert!(tiles.uploads.bytes <= limit);
        assert!(tiles.uploads.images.len() < usize::from(columns) * usize::from(rows));
        let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&png))
            .ok_or(crate::Error::DecodeImage)?;
        let info = skia_safe::ImageInfo::new(
            (400, 240),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            skia_safe::ColorSpace::new_srgb(),
        );
        let mut pixels = vec![0_u8; 400 * 240 * 4];
        assert!(image.read_pixels(
            &info,
            &mut pixels,
            400 * 4,
            (0, 0),
            skia_safe::image::CachingHint::Disallow
        ));
        for y in 0..usize::from(rows) {
            for x in 0..usize::from(columns) {
                let offset = ((y * 4 + 2) * 400 + x * 4 + 2) * 4;
                let pixel = &pixels[offset..offset + 4];
                let dominant = if (x + y) % 2 == 0 { 0 } else { 2 };
                assert!(
                    pixel[dominant] >= 250 && pixel[2 - dominant] < 5,
                    "wrong tile at{x},{y}: {pixel:?}"
                );
            }
        }
        Ok(())
    }
}
