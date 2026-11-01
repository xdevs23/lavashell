// SPDX-License-Identifier: GPL-3.0-only
mod geometry;
mod paragraph;

use super::{Configuration, TextDestination, TextRendering};
use crate::failure::{DrawFailure, Failures};
use crate::text_cache::{CachedParagraph, TextCache};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
pub(crate) use paragraph::ParagraphInput;
use skia_safe::textlayout::{FontCollection, Paragraph};

/// Shapes and measures text with the fonts in a font registry, without a GPU.
pub struct Shaper {
    fonts: FontCollection,
    private_fonts: FontCollection,
    registry: crate::Fonts,
    paragraphs: TextCache,
    text_configuration: State<Configuration>,
    failures: Failures,
}

impl Shaper {
    pub(crate) fn new(registry: &crate::Fonts, failures: Failures) -> Result<Self, crate::Error> {
        let mut private_fonts = registry.collection()?;
        // SkParagraph has its own paragraph cache next to the Rust paragraph cache. Private text uses a separate font
        // collection with the SkParagraph cache turned off, so private text enters neither cache.
        private_fonts.paragraph_cache_mut().turn_on(false);
        Ok(Self {
            fonts: registry.collection()?,
            private_fonts,
            registry: registry.clone(),
            paragraphs: TextCache::default(),
            text_configuration: State::new(Configuration::default()),
            failures,
        })
    }

    /// Sets the text rendering policy, adjusted by [`TextRendering::for_destination`], and where the text is drawn.
    /// Returns whether the policy or the destination changed.
    pub(crate) fn set_text_rendering(
        &mut self,
        rendering: TextRendering,
        destination: TextDestination,
    ) -> bool {
        let configuration = Configuration {
            rendering: rendering.for_destination(destination),
            destination,
        };
        let changed = self.text_configuration.peek() != configuration;
        if changed {
            // The key of the shaping cache inside SkParagraph does not contain the font edging, the hinting or the
            // fractional positioning. The runs shaped with the previous policy are therefore discarded too.
            self.fonts.paragraph_cache_mut().reset();
            self.paragraphs.clear();
            self.text_configuration.set(configuration);
        }
        changed
    }

    pub(crate) fn configuration(&self) -> Configuration {
        self.text_configuration.peek()
    }

    pub(crate) fn tracked_configuration(&self) -> Configuration {
        self.text_configuration.get()
    }

    pub(crate) fn begin_paint(&mut self, surface: SurfaceId) {
        self.paragraphs.begin_paint(surface);
    }

    pub(crate) fn forget_surface(&mut self, surface: SurfaceId) {
        self.paragraphs.forget_surface(surface);
    }

    pub(crate) fn paragraph_input<'t>(
        &self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
    ) -> Result<ParagraphInput<'t>, DrawFailure> {
        ParagraphInput::new(
            text,
            style,
            width,
            self.text_configuration.get().rendering,
            self.fonts.clone(),
        )
    }

    pub(crate) fn cached(&mut self, input: ParagraphInput<'_>) -> &mut CachedParagraph {
        input.cached(&mut self.paragraphs, &self.registry)
    }

    /// A paragraph of private text, shaped outside every paragraph cache.
    pub(crate) fn private_paragraph(
        &self,
        rendering: TextRendering,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<Paragraph, DrawFailure> {
        let layout = paragraph::Layout::new(style, width)?;
        Ok(paragraph::build(
            &self.registry,
            self.private_fonts.clone(),
            rendering,
            text,
            style,
            &layout,
        ))
    }

    fn prepare_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<&mut Paragraph, DrawFailure> {
        let input = self.paragraph_input(text, style, width)?;
        Ok(&mut self.cached(input).shaped.paragraph)
    }
}

#[cfg(test)]
impl Shaper {
    pub(crate) fn paragraphs(&mut self) -> &mut TextCache {
        &mut self.paragraphs
    }

    pub(crate) fn skia_cache_count(&mut self) -> i32 {
        self.fonts.paragraph_cache_mut().count()
    }

    pub(crate) fn private_skia_cache_count(&mut self) -> i32 {
        self.private_fonts.paragraph_cache_mut().count()
    }
}
