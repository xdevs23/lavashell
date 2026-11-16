// SPDX-License-Identifier: GPL-3.0-only
use crate::fonts::LoadedFonts;
use skia_safe::textlayout::FontCollection;

pub(crate) struct Sources {
    fonts: FontCollection,
    rendering: Option<crate::text::TextRendering>,
}

impl Sources {
    pub fn new(loaded_fonts: &LoadedFonts) -> Self {
        Self {
            fonts: loaded_fonts.collection(),
            rendering: None,
        }
    }

    pub fn collection(&mut self, rendering: crate::text::TextRendering) -> FontCollection {
        if self.rendering != Some(rendering) {
            self.fonts.paragraph_cache_mut().reset();
            self.rendering = Some(rendering);
        }
        self.fonts.clone()
    }
}
