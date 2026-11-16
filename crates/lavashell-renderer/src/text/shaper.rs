// SPDX-License-Identifier: GPL-3.0-only
mod coordinate_fonts;
mod crossfade;
mod geometry;
mod paragraph;
mod sources;

use super::{Configuration, ParagraphDrawer, ParagraphRequest, TextDestination, TextRendering};
use crate::failure::{DrawFailure, Failures};
use crate::fonts::LoadedFonts;
use crate::text_cache::{CachedParagraph, TextCache};
use coordinate_fonts::CoordinateFonts;
pub(crate) use crossfade::{FaceShare, WeightRendering};
use lavashell_core::prelude::*;
use lavashell_core::surface::{PerSurface, SurfaceId};
pub(crate) use paragraph::ParagraphInput;
use skia_safe::textlayout::{FontCollection, Paragraph};

pub struct Shaper {
    fonts: FontCollection,
    private_fonts: FontCollection,
    source_fonts: sources::Sources,
    loaded_fonts: LoadedFonts,
    coordinate_fonts: CoordinateFonts,
    paragraphs: TextCache,
    text_configuration: State<Configuration>,
    failures: Failures,
}

impl Shaper {
    pub(crate) fn new(loaded_fonts: LoadedFonts, failures: Failures) -> Self {
        Self {
            fonts: loaded_fonts.collection(),
            private_fonts: loaded_fonts.uncached_collection(),
            source_fonts: sources::Sources::new(&loaded_fonts),
            loaded_fonts,
            coordinate_fonts: CoordinateFonts::default(),
            paragraphs: TextCache::default(),
            text_configuration: State::new(Configuration::default()),
            failures,
        }
    }

    pub fn with_rendering(
        registry: &crate::Fonts,
        rendering: TextRendering,
    ) -> Result<Self, crate::Error> {
        let shaper = Self::new(registry.load()?, Failures::default());
        shaper.text_configuration.set(Configuration {
            rendering,
            destination: TextDestination::default(),
        });
        Ok(shaper)
    }

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
        self.coordinate_fonts.begin_paint(surface);
    }

    pub(crate) fn paragraph_input<'t>(
        &self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
        rendering: TextRendering,
    ) -> Result<ParagraphInput<'t>, DrawFailure> {
        ParagraphInput::new(text, style, width, rendering, self.fonts.clone())
    }

    pub(crate) fn cached(&mut self, input: ParagraphInput<'_>) -> &mut CachedParagraph {
        input.cached(
            &mut self.paragraphs,
            &self.loaded_fonts,
            &mut self.coordinate_fonts,
        )
    }

    pub(crate) fn source_paragraph_input<'t>(
        &mut self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
        rendering: TextRendering,
    ) -> Result<ParagraphInput<'t>, DrawFailure> {
        self.observe_rendering();
        ParagraphInput::new(
            text,
            style,
            width,
            rendering,
            self.source_fonts.collection(rendering),
        )
    }

    pub(crate) fn requested_paragraph(
        &mut self,
        text: &str,
        request: &ParagraphRequest<'_>,
    ) -> Result<&mut CachedParagraph, DrawFailure> {
        let ParagraphRequest {
            drawer,
            rendering,
            style,
            width,
        } = *request;
        let input = match drawer {
            ParagraphDrawer::Rasterizer => {
                self.source_paragraph_input(text, style, width, rendering)?
            }
            ParagraphDrawer::Canvas => self.paragraph_input(text, style, width, rendering)?,
        };
        Ok(self.cached(input))
    }

    pub(crate) fn private_paragraph(
        &mut self,
        rendering: TextRendering,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<Paragraph, DrawFailure> {
        let layout = paragraph::Layout::new(style, width)?;
        let fonts = paragraph::ParagraphFonts {
            loaded_fonts: &self.loaded_fonts,
            named_weights: self.private_fonts.clone(),
            coordinates: &mut self.coordinate_fonts,
        };
        Ok(paragraph::build(fonts, rendering, text, style, &layout))
    }

    #[cfg(test)]
    pub(crate) fn prepare_source_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
        rendering: TextRendering,
    ) -> Result<&mut Paragraph, DrawFailure> {
        let input = self.source_paragraph_input(text, style, width, rendering)?;
        Ok(&mut self.cached(input).shaped.paragraph)
    }

    fn prepare_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<&mut Paragraph, DrawFailure> {
        let rendering = self.tracked_configuration().rendering;
        let input = self.paragraph_input(text, style, width, rendering)?;
        Ok(&mut self.cached(input).shaped.paragraph)
    }
}

impl PerSurface for Shaper {
    fn forget_surface(&mut self, surface: SurfaceId) {
        self.paragraphs.forget_surface(surface);
        self.coordinate_fonts.forget_surface(surface);
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
