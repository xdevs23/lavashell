// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::text::{FaceParagraphs, PaintableParagraph, PaintableText, ParagraphRequest, Shaper};
use crate::text_cache::ShapedParagraph;
use lavashell_core::protected_text::PrivateText;

impl PaintableText for PrivateText {
    fn with_paragraph<R>(
        &self,
        shaper: &mut Shaper,
        request: &ParagraphRequest<'_>,
        paint: impl FnOnce(PaintableParagraph<'_>) -> Result<R, DrawFailure>,
    ) -> Result<R, DrawFailure> {
        let paragraph = self.expose(|value| {
            shaper.private_paragraph(request.rendering, value, request.style, request.width)
        })?;
        paint(PaintableParagraph {
            paragraph: &mut ShapedParagraph::new(paragraph),
            key: None,
        })
    }

    fn face_paragraphs<'s>(
        &'s self,
        shaper: &'s mut Shaper,
        [lighter, heavier]: [ParagraphRequest<'s>; 2],
    ) -> Result<FaceParagraphs<'s>, DrawFailure> {
        let mut face_paragraph = |value: &str, request: ParagraphRequest<'_>| {
            (shaper.private_paragraph(request.rendering, value, request.style, request.width))
                .map(ShapedParagraph::new)
        };
        self.expose(|value| {
            Ok(FaceParagraphs::Built([
                face_paragraph(value, lighter)?,
                face_paragraph(value, heavier)?,
            ]))
        })
    }
}

#[cfg(test)]
mod tests;
