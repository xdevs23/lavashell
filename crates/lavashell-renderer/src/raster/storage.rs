// SPDX-License-Identifier: GPL-3.0-only
use super::RasterCache;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TileStorage {
    Retained,
    Transient,
}

struct Storage<'a> {
    cache: &'a mut RasterCache,
    previous: TileStorage,
}

impl Drop for Storage<'_> {
    fn drop(&mut self) {
        self.cache.storage = self.previous;
    }
}

impl RasterCache {
    pub fn without_storage<R>(&mut self, draw: impl FnOnce(&mut Self) -> R) -> R {
        let previous = std::mem::replace(&mut self.storage, TileStorage::Transient);
        let scope = Storage {
            cache: self,
            previous,
        };
        draw(&mut *scope.cache)
    }
}

#[cfg(test)]
mod tests {
    use super::super::RasterMode;
    use super::super::tests::spec;
    use super::*;
    use crate::failure::Failures;
    use crate::test_error::TestError;
    use crate::test_surfaces::unpremultiplied_pixel;
    use skia_safe::{Color, Rect};

    #[test]
    fn transient_storage_neither_reuses_nor_retains_pixels_of_one_key() -> Result<(), TestError> {
        let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
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
                assert_eq!(
                    unpremultiplied_pixel(&mut surface, 2, 2),
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
        let mut cache = RasterCache::new(RasterMode::Tiles, Failures::default());
        cache.without_storage(|cache| {
            assert_eq!(cache.storage, TileStorage::Transient);
            cache.without_storage(|cache| assert_eq!(cache.storage, TileStorage::Transient));
            assert_eq!(cache.storage, TileStorage::Transient);
        });
        assert_eq!(cache.storage, TileStorage::Retained);
    }
}
