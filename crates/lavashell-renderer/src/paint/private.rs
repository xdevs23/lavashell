// SPDX-License-Identifier: GPL-3.0-only
use super::Painter;
use crate::failure::DrawFailure;
use crate::text::{Drawn, RasterInput};
use crate::text_cache::ShapedParagraph;
use lavashell_core::{prelude::*, protected_text::PrivateText};
use skia_safe::Canvas;

impl Painter {
    pub(super) fn paint_private_text(
        &mut self,
        canvas: &Canvas,
        text: &PrivateText,
        rect: Rect,
        style: &TextStyle<Dp>,
    ) -> Result<(), DrawFailure> {
        let configuration = self.shaper.tracked_configuration();
        text.expose(|value| {
            let paragraph =
                self.shaper
                    .private_paragraph(configuration.rendering, value, style, rect.width)?;
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
                    let mut source = ShapedParagraph::new(self.shaper.private_paragraph(
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
