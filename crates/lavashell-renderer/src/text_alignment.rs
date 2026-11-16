// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::{Dp, ParagraphDirection, TextAlignment, TextStyle};
use skia_safe::textlayout::{ParagraphStyle, TextAlign, TextDirection};

pub(crate) fn apply(style: &TextStyle<Dp>, paragraph: &mut ParagraphStyle) {
    let _chained_paragraph_style = paragraph.set_text_align(match style.alignment {
        TextAlignment::Start => TextAlign::Start,
        TextAlignment::Center => TextAlign::Center,
        TextAlignment::End => TextAlign::End,
    });
    let _chained_paragraph_style = paragraph.set_text_direction(match style.direction {
        ParagraphDirection::LeftToRight => TextDirection::LTR,
        ParagraphDirection::RightToLeft => TextDirection::RTL,
    });
}

#[cfg(test)]
mod tests;
