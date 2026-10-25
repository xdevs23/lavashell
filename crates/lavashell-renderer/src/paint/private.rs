// SPDX-License-Identifier: GPL-3.0-only
use super::{Painter, paragraph};
use crate::failure::DrawFailure;
use crate::text::{Drawn, RasterInput};
use crate::text_cache::ShapedParagraph;
use lavashell_core::{prelude::*, protected_text::PrivateText};
use skia_safe::{
    Canvas,
    textlayout::{FontCollection, Paragraph},
};

pub(super) struct Private {
    fonts: FontCollection,
}

impl Private {
    pub fn new(registry: &crate::Fonts) -> Result<Self, crate::Error> {
        let mut fonts = registry.collection()?;
        // SkParagraph has its own paragraph cache next to the Rust paragraph cache. Private text uses a separate font
        // collection with the SkParagraph cache turned off, so private text enters neither cache.
        fonts.paragraph_cache_mut().turn_on(false);
        Ok(Self { fonts })
    }

    pub fn paragraph(
        &self,
        registry: &crate::Fonts,
        rendering: crate::text::TextRendering,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<Paragraph, DrawFailure> {
        let layout = paragraph::Layout::new(style, width)?;
        Ok(paragraph::build(
            registry,
            self.fonts.clone(),
            rendering,
            text,
            style,
            &layout,
        ))
    }
}

impl Painter {
    pub(super) fn paint_private_text(
        &mut self,
        canvas: &Canvas,
        text: &PrivateText,
        rect: Rect,
        style: &TextStyle<Dp>,
    ) -> Result<(), DrawFailure> {
        let configuration = self.text_configuration.get();
        text.expose(|value| {
            let paragraph = self.private_text.paragraph(
                &self.registry,
                configuration.rendering,
                value,
                style,
                rect.width,
            )?;
            let origin = crate::text::centered(rect, &paragraph)?;
            let input = RasterInput {
                canvas,
                policy: configuration.rendering,
                destination: configuration.destination,
                opaque_scope: self.text_scopes.opaque(),
                style,
                foreground: super::color4f(style.color)?,
            };
            let providers = &self.text_rasterizers;
            self.resources.tiles.get_mut().without_storage(|tiles| {
                if let Some((provider, source_style, source_rendering)) = providers.source(&input) {
                    let mut source = ShapedParagraph::new(self.private_text.paragraph(
                        &self.registry,
                        source_rendering,
                        value,
                        &source_style,
                        rect.width,
                    )?);
                    match providers.draw(provider, &input, origin, &mut source, 0, tiles) {
                        Ok(Drawn::Painted) => return Ok(()),
                        Ok(Drawn::NotApplicable) => {}
                        Err(error) => self
                            .failures
                            .record_caused(DrawFailure::SubpixelText, &error),
                    }
                }
                tiles.paragraph(canvas, &mut ShapedParagraph::new(paragraph), 1, origin);
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests;
