// SPDX-License-Identifier: GPL-3.0-only
use skia_safe::textlayout::FontCollection;

pub(crate) struct Sources {
    fonts: FontCollection,
    rendering: Option<crate::text::TextRendering>,
}

impl Sources {
    pub fn new(registry: &crate::Fonts) -> Result<Self, crate::Error> {
        Ok(Self {
            fonts: registry.collection()?,
            rendering: None,
        })
    }

    pub fn collection(&mut self, rendering: crate::text::TextRendering) -> FontCollection {
        if self.rendering != Some(rendering) {
            self.fonts.paragraph_cache_mut().reset();
            self.rendering = Some(rendering);
        }
        self.fonts.clone()
    }
}
