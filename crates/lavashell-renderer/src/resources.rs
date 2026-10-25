// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::field::{FieldRoom, Fields};
use crate::raster::{RasterCache, resource_cache_limit};
use lavashell_core::surface::SurfaceId;
use skia_safe::Canvas;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) struct PaintResources {
    pub fields: Fields,
    pub tiles: RefCell<RasterCache>,
    pub held: Rc<HeldImages>,
}

/// A cache in the painter that stores images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageCache {
    Packets,
    Backdrops,
}

/// The bytes of the images stored by each image cache of a painter. The caches share the resource cache limit of the
/// canvas they draw on.
#[derive(Default)]
pub(crate) struct HeldImages {
    packets: Cell<usize>,
    backdrops: Cell<usize>,
}

impl HeldImages {
    pub fn set(&self, cache: ImageCache, bytes: usize) {
        self.part(cache).set(bytes);
    }

    /// The most bytes that `cache` may store on `canvas`: the resource cache limit of `canvas` minus the bytes stored
    /// by the other image cache.
    pub fn budget(&self, cache: ImageCache, canvas: &Canvas) -> usize {
        let other = match cache {
            ImageCache::Packets => ImageCache::Backdrops,
            ImageCache::Backdrops => ImageCache::Packets,
        };
        resource_cache_limit(canvas).saturating_sub(self.part(other).get())
    }

    fn part(&self, cache: ImageCache) -> &Cell<usize> {
        match cache {
            ImageCache::Packets => &self.packets,
            ImageCache::Backdrops => &self.backdrops,
        }
    }
}

impl PaintResources {
    pub fn new(room: &FieldRoom) -> Result<Self, Error> {
        Ok(Self {
            fields: Fields::new(room)?,
            tiles: RefCell::default(),
            held: Rc::default(),
        })
    }

    /// Begins a frame of `surface` and drops the field data and raster tiles that no recent frame of any surface used.
    pub fn begin_frame(&mut self, surface: SurfaceId) {
        self.fields.begin_frame(surface);
        self.tiles.get_mut().begin_frame(surface);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.fields.forget_surface(surface);
        self.tiles.get_mut().forget_surface(surface);
    }

    pub fn retire(&mut self) {
        self.fields.clear_textures();
        self.tiles.get_mut().clear();
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
