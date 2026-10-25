// SPDX-License-Identifier: GPL-3.0-only
use super::RasterCache;

struct Storage<'a> {
    cache: &'a mut RasterCache,
    previous: bool,
}

impl Drop for Storage<'_> {
    fn drop(&mut self) {
        self.cache.without_storage = self.previous;
    }
}

impl RasterCache {
    /// Runs `draw` with tile storage turned off. Tiles rasterized in `draw` count against the upload limit, but the
    /// cache neither reuses stored tiles nor stores new ones.
    pub fn without_storage<R>(&mut self, draw: impl FnOnce(&mut Self) -> R) -> R {
        let previous = std::mem::replace(&mut self.without_storage, true);
        let scope = Storage {
            cache: self,
            previous,
        };
        draw(&mut *scope.cache)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::spec;
    use super::*;
    use crate::test_error::TestError;
    use skia_safe::{Color, Rect};

    #[test]
    fn transient_storage_neither_reuses_nor_retains_pixels_of_one_key() -> Result<(), TestError> {
        let mut cache = RasterCache::new(true);
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((8, 8)).ok_or(TestError::RasterSurface)?;
        let rect = Rect::from_xywh(0.0, 0.0, 8.0, 8.0);
        cache.draw_tile(surface.canvas(), &spec(7, 256, (8, 8), rect), |tile| {
            let _chained_canvas = tile.clear(Color::WHITE);
        })?;
        let ids: Vec<_> = cache.images().map(skia_safe::Image::unique_id).collect();
        cache.without_storage(|cache| -> Result<(), TestError> {
            for color in [Color::RED, Color::BLUE] {
                cache.draw_tile(surface.canvas(), &spec(7, 256, (8, 8), rect), |tile| {
                    let _chained_canvas = tile.clear(color);
                })?;
                let info = skia_safe::ImageInfo::new(
                    (1, 1),
                    skia_safe::ColorType::RGBA8888,
                    skia_safe::AlphaType::Unpremul,
                    None,
                );
                let mut pixel = [0; 4];
                assert!(surface.read_pixels(&info, &mut pixel, 4, (2, 2)));
                assert_eq!(
                    pixel,
                    if color == Color::RED {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }
                );
                assert_eq!(
                    cache
                        .images()
                        .map(skia_safe::Image::unique_id)
                        .collect::<Vec<_>>(),
                    ids
                );
            }
            Ok(())
        })?;
        assert_eq!(cache.images().count(), 1);
        let mut rasterized = false;
        cache.draw_tile(surface.canvas(), &spec(7, 256, (8, 8), rect), |_| {
            rasterized = true;
        })?;
        assert!(!rasterized, "the ordinary cache keeps its pixels");
        Ok(())
    }

    #[test]
    fn nested_transient_scopes_restore_storage() {
        let mut cache = RasterCache::new(true);
        cache.without_storage(|cache| {
            assert!(cache.without_storage);
            cache.without_storage(|cache| assert!(cache.without_storage));
            assert!(cache.without_storage);
        });
        assert!(!cache.without_storage);
    }
}
