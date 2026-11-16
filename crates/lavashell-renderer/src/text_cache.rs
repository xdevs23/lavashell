// SPDX-License-Identifier: GPL-3.0-only
use crate::raster::ink_bounds;
use crate::retention::{Frames, Retained};
use crate::text::{Glyphs, TextRendering};
use lavashell_core::scene::TextStyle;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Dp;
use skia_safe::{Rect, textlayout::Paragraph};

#[derive(Default)]
pub(crate) struct TextCache {
    paragraphs: Retained<u64, CachedParagraph>,
    frames: Frames,
    built: u64,
}

impl TextCache {
    pub fn begin_paint(&mut self, surface: SurfaceId) {
        self.frames.begin(surface);
        self.paragraphs.retain_recent(&self.frames);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.frames.forget(surface);
    }

    pub fn clear(&mut self) {
        self.paragraphs = Retained::default();
        self.frames = Frames::default();
    }

    pub fn get(
        &mut self,
        key: u64,
        is_wanted: impl FnOnce(&ParagraphSource) -> bool,
        build: impl FnOnce() -> (ParagraphSource, Paragraph),
    ) -> &mut CachedParagraph {
        let built_count = &mut self.built;
        let fits = |cached: &CachedParagraph| is_wanted(&cached.source);
        self.paragraphs
            .get_fitting_or_make(key, &self.frames, fits, || {
                let (source, paragraph) = build();
                *built_count += 1;
                CachedParagraph {
                    key: *built_count,
                    source,
                    shaped: ShapedParagraph::new(paragraph),
                }
            })
    }
}

pub(crate) struct ParagraphSource {
    pub text: String,
    pub style: TextStyle<Dp>,
    pub width: Dp,
    pub rendering: TextRendering,
}

pub(crate) struct CachedParagraph {
    pub key: u64,
    source: ParagraphSource,
    pub shaped: ShapedParagraph,
}

pub(crate) struct ShapedParagraph {
    pub paragraph: Paragraph,
    glyphs: Option<Glyphs>,
    ink: Option<Rect>,
}

impl ShapedParagraph {
    pub fn new(paragraph: Paragraph) -> Self {
        Self {
            paragraph,
            glyphs: None,
            ink: None,
        }
    }

    pub fn glyphs(&mut self) -> Glyphs {
        *self
            .glyphs
            .get_or_insert_with(|| Glyphs::of(&mut self.paragraph))
    }

    pub fn ink(&mut self) -> Rect {
        *self
            .ink
            .get_or_insert_with(|| ink_bounds(&mut self.paragraph))
    }
}

#[cfg(test)]
mod tests;
