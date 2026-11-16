// SPDX-License-Identifier: GPL-3.0-only
use crate::fonts::LoadedFonts;
use crate::retention::{Frames, Retained};
use lavashell_core::surface::SurfaceId;
use skia_safe::font_style::Weight;
use skia_safe::textlayout::FontCollection;

#[derive(Default)]
pub(super) struct CoordinateFonts {
    collections: Retained<i32, FontCollection>,
    frames: Frames,
}

impl CoordinateFonts {
    pub fn begin_paint(&mut self, surface: SurfaceId) {
        self.frames.begin(surface);
        self.collections.retain_recent(&self.frames);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.frames.forget(surface);
    }

    pub fn collection(&mut self, loaded_fonts: &LoadedFonts, whole: Weight) -> FontCollection {
        (self.collections)
            .get_or_make(*whole, &self.frames, |_whole| {
                loaded_fonts.uncached_collection()
            })
            .clone()
    }
}
