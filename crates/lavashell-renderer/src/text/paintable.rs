// SPDX-License-Identifier: GPL-3.0-only
use super::{Shaper, TextRendering};
use crate::failure::DrawFailure;
use crate::text_cache::ShapedParagraph;
use lavashell_core::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum ParagraphDrawer {
    Rasterizer,
    Canvas,
}

#[derive(Clone, Copy)]
pub(crate) struct ParagraphRequest<'a> {
    pub drawer: ParagraphDrawer,
    pub rendering: TextRendering,
    pub style: &'a TextStyle<Dp>,
    pub width: Dp,
}

pub(crate) struct PaintableParagraph<'a> {
    pub paragraph: &'a mut ShapedParagraph,
    pub key: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) enum Face {
    Lighter,
    Heavier,
}

pub(crate) enum FaceParagraphs<'s> {
    Cached {
        shaper: &'s mut Shaper,
        text: &'s str,
        requests: [ParagraphRequest<'s>; 2],
    },
    Built([ShapedParagraph; 2]),
}

impl FaceParagraphs<'_> {
    pub fn paragraph(&mut self, face: Face) -> Result<PaintableParagraph<'_>, DrawFailure> {
        let index = match face {
            Face::Lighter => 0,
            Face::Heavier => 1,
        };
        match self {
            Self::Cached {
                shaper,
                text,
                requests,
            } => cached_paragraph(shaper, text, &requests[index]),
            Self::Built(paragraphs) => Ok(PaintableParagraph {
                paragraph: &mut paragraphs[index],
                key: None,
            }),
        }
    }
}

pub(crate) trait PaintableText {
    fn with_paragraph<R>(
        &self,
        shaper: &mut Shaper,
        request: &ParagraphRequest<'_>,
        paint: impl FnOnce(PaintableParagraph<'_>) -> Result<R, DrawFailure>,
    ) -> Result<R, DrawFailure>;

    fn face_paragraphs<'s>(
        &'s self,
        shaper: &'s mut Shaper,
        requests: [ParagraphRequest<'s>; 2],
    ) -> Result<FaceParagraphs<'s>, DrawFailure>;
}

impl PaintableText for str {
    fn with_paragraph<R>(
        &self,
        shaper: &mut Shaper,
        request: &ParagraphRequest<'_>,
        paint: impl FnOnce(PaintableParagraph<'_>) -> Result<R, DrawFailure>,
    ) -> Result<R, DrawFailure> {
        paint(cached_paragraph(shaper, self, request)?)
    }

    fn face_paragraphs<'s>(
        &'s self,
        shaper: &'s mut Shaper,
        requests: [ParagraphRequest<'s>; 2],
    ) -> Result<FaceParagraphs<'s>, DrawFailure> {
        Ok(FaceParagraphs::Cached {
            shaper,
            text: self,
            requests,
        })
    }
}

fn cached_paragraph<'s>(
    shaper: &'s mut Shaper,
    text: &str,
    request: &ParagraphRequest<'_>,
) -> Result<PaintableParagraph<'s>, DrawFailure> {
    let cached = shaper.requested_paragraph(text, request)?;
    Ok(PaintableParagraph {
        paragraph: &mut cached.shaped,
        key: Some(cached.key),
    })
}
