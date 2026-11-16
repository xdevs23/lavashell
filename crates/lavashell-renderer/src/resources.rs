// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::failure::Failures;
use crate::field::{FieldRoom, Fields};
use crate::raster::{RasterCache, RasterMode, resource_cache_limit};
use lavashell_core::surface::{PerSurface, SurfaceId};
use skia_safe::Canvas;
use std::cell::{Cell, RefCell};

pub(crate) struct PaintResources {
    pub fields: Fields,
    pub tiles: RefCell<RasterCache>,
    pub held: HeldImages,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageCache {
    Packets,
    Backdrops,
}

impl ImageCache {
    const ALL: [Self; 2] = [Self::Packets, Self::Backdrops];
}

#[derive(Default)]
pub(crate) struct HeldImages {
    packets: Cell<usize>,
    backdrops: Cell<usize>,
}

impl HeldImages {
    pub fn set(&self, cache: ImageCache, bytes: usize) {
        self.part(cache).set(bytes);
    }

    pub fn budget(&self, cache: ImageCache, canvas: &Canvas) -> usize {
        let held_by_others = (ImageCache::ALL.into_iter())
            .filter(|other| *other != cache)
            .map(|other| self.part(other).get())
            .fold(0, usize::saturating_add);
        resource_cache_limit(canvas).saturating_sub(held_by_others)
    }

    fn part(&self, cache: ImageCache) -> &Cell<usize> {
        match cache {
            ImageCache::Packets => &self.packets,
            ImageCache::Backdrops => &self.backdrops,
        }
    }
}

impl PaintResources {
    pub fn new(room: &FieldRoom, failures: Failures) -> Result<Self, Error> {
        Ok(Self {
            fields: Fields::new(room)?,
            tiles: RefCell::new(RasterCache::new(RasterMode::Vector, failures)),
            held: HeldImages::default(),
        })
    }

    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.fields.begin_frame(surface);
        self.tiles.get_mut().begin_frame(surface);
    }

    pub fn retire(&mut self) {
        self.fields.clear_textures();
        self.tiles.get_mut().clear();
    }
}

impl PerSurface for PaintResources {
    fn forget_surface(&mut self, surface: SurfaceId) {
        self.fields.forget_surface(surface);
        self.tiles.get_mut().forget_surface(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn an_image_cache_gets_the_limit_less_what_the_other_cache_holds() -> Result<(), TestError> {
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((1, 1)).ok_or(TestError::RasterSurface)?;
        let canvas = surface.canvas();
        let limit = resource_cache_limit(canvas);
        let held = HeldImages::default();
        held.set(ImageCache::Packets, 3);
        assert_eq!(held.budget(ImageCache::Packets, canvas), limit);
        assert_eq!(held.budget(ImageCache::Backdrops, canvas), limit - 3);
        held.set(ImageCache::Backdrops, limit);
        assert_eq!(held.budget(ImageCache::Packets, canvas), 0);
        Ok(())
    }
}
