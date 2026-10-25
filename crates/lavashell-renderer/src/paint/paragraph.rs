// SPDX-License-Identifier: GPL-3.0-only
use super::solid;
use crate::failure::DrawFailure;
use crate::scalar::scalars;
use crate::text_cache::{CachedParagraph, ParagraphSource, TextCache};
use lavashell_core::prelude::*;
use num_traits::AsPrimitive;
use skia_safe::{
    FontStyle, Paint,
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextStyle as SkTextStyle,
    },
};
use std::hash::{Hash, Hasher};

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

/// The numbers of a paragraph converted to Skia scalars, and the paint of its text, as Skia uses them for the layout.
pub(super) struct Layout {
    size: f32,
    letter_spacing: f32,
    width: f32,
    foreground: Paint,
}

impl Layout {
    /// # Errors
    /// Returns [`DrawFailure::NumberRange`] when a finite number of `style` or `width` lies beyond the range of
    /// Skia scalars.
    pub fn new(style: &TextStyle<Dp>, width: Dp) -> Result<Self, DrawFailure> {
        let letter_spacing = style.letter_spacing.to_dp(style.size);
        let [size, letter_spacing, width] =
            scalars([style.size.get(), letter_spacing.get(), width.get()])?;
        Ok(Self {
            size,
            letter_spacing,
            width,
            foreground: solid(style.color)?,
        })
    }
}

/// The inputs that a cached paragraph is built from, and their hash, which is the cache key of the paragraph.
pub(crate) struct ParagraphInput<'t> {
    text: &'t str,
    style: &'t TextStyle<Dp>,
    width: Dp,
    layout: Layout,
    rendering: crate::text::TextRendering,
    fonts: FontCollection,
    key: u64,
}

impl<'t> ParagraphInput<'t> {
    /// # Errors
    /// Returns [`DrawFailure::NumberRange`] when a finite number of `style` or `width` lies beyond the range of
    /// Skia scalars.
    pub fn new(
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
        rendering: crate::text::TextRendering,
        fonts: FontCollection,
    ) -> Result<Self, DrawFailure> {
        let layout = Layout::new(style, width)?;
        let mut key = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut key);
        style.family.hash(&mut key);
        style.size.get().to_bits().hash(&mut key);
        style.color.hash_components(&mut key);
        style.weight.hash(&mut key);
        style.max_lines.hash(&mut key);
        style.letter_spacing.get().to_bits().hash(&mut key);
        style.spans.hash(&mut key);
        style.alignment.hash(&mut key);
        style.direction.hash(&mut key);
        width.get().to_bits().hash(&mut key);
        rendering.hash(&mut key);
        Ok(Self {
            text,
            style,
            width,
            layout,
            rendering,
            fonts,
            key: key.finish(),
        })
    }

    /// The paragraph stored in `paragraphs`, built first when it is missing. Another paragraph stored under the
    /// same hash is replaced.
    pub fn cached<'c>(
        self,
        paragraphs: &'c mut TextCache,
        registry: &crate::Fonts,
    ) -> &'c mut CachedParagraph {
        let make = || {
            let paragraph = build(
                registry,
                self.fonts.clone(),
                self.rendering,
                self.text,
                self.style,
                &self.layout,
            );
            (self.source(), paragraph)
        };
        paragraphs.get(self.key, |source| self.is_source(source), make)
    }

    fn source(&self) -> ParagraphSource {
        ParagraphSource {
            text: self.text.to_owned(),
            style: self.style.clone(),
            width: self.width,
            rendering: self.rendering,
        }
    }

    fn is_source(&self, source: &ParagraphSource) -> bool {
        let ParagraphSource {
            text,
            style,
            width,
            rendering,
        } = source;
        self.text == text.as_str()
            && self.style == style
            && self.width == *width
            && self.rendering == *rendering
    }
}

pub(super) fn ellipsis(style: &TextStyle<Dp>) -> Option<&'static str> {
    style.max_lines.map(|_| lavashell_core::scene::ELLIPSIS)
}

/// The whole Skia weight nearest to `weight`, as Skia matches fonts by whole weights. A weight lies within 1 to
/// 1000, so the rounded weight converts to an `i32` exactly.
fn skia_weight(weight: FontWeight) -> skia_safe::font_style::Weight {
    let whole: i32 = weight.get().round().as_();
    skia_safe::font_style::Weight::from(whole)
}

pub(super) fn build(
    registry: &crate::Fonts,
    fonts: FontCollection,
    rendering: crate::text::TextRendering,
    text: &str,
    style: &TextStyle<Dp>,
    layout: &Layout,
) -> Paragraph {
    let mut ts = SkTextStyle::new();
    let _chained_style = ts
        .set_font_size(layout.size)
        .set_foreground_paint(&layout.foreground)
        .set_font_families(&registry.families(&style.family));
    let _chained_style = ts.set_font_style(FontStyle::new(
        skia_weight(style.weight),
        skia_safe::font_style::Width::NORMAL,
        skia_safe::font_style::Slant::Upright,
    ));
    let _chained_style = ts.set_letter_spacing(layout.letter_spacing);
    let mut ps = ParagraphStyle::new();
    rendering.apply(&mut ts, &mut ps);
    crate::text_alignment::apply(style, &mut ps);
    let _chained_paragraph_style = ps.set_text_style(&ts).set_max_lines(style.max_lines);
    if let Some(ellipsis) = ellipsis(style) {
        let _chained_paragraph_style = ps.set_ellipsis(ellipsis);
    }
    let mut builder = ParagraphBuilder::new(&ps, fonts);
    let mut cursor = 0;
    for span in &style.spans {
        if span.range.start < cursor
            || span.range.end > text.len()
            || !text.is_char_boundary(span.range.start)
            || !text.is_char_boundary(span.range.end)
            || span.range.start >= span.range.end
        {
            continue;
        }
        let _chained_builder = builder.add_text(&text[cursor..span.range.start]);
        let mut inline = ts.clone();
        let _chained_style = inline.set_font_style(FontStyle::new(
            skia_weight(span.weight.unwrap_or(style.weight)),
            skia_safe::font_style::Width::NORMAL,
            if span.italic {
                skia_safe::font_style::Slant::Italic
            } else {
                skia_safe::font_style::Slant::Upright
            },
        ));
        if let Some(family) = &span.family {
            let _chained_style = inline.set_font_families(&registry.families(family));
        }
        let _chained_builder = builder.push_style(&inline);
        let _chained_builder = builder.add_text(&text[span.range.clone()]);
        let _chained_builder = builder.pop();
        cursor = span.range.end;
    }
    let _chained_builder = builder.add_text(&text[cursor..]);
    let mut paragraph = builder.build();
    paragraph.layout(layout.width);
    paragraph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float_asserts::same_at;
    use crate::test_error::TestError;

    impl ParagraphInput<'_> {
        pub(crate) fn key(&self) -> u64 {
            self.key
        }
    }

    #[test]
    fn letter_spacing_in_em_adds_the_fraction_of_the_font_size_to_every_glyph()
    -> Result<(), TestError> {
        const TEXT: &str = "abc";
        let glyphs = f32::from(u16::try_from(TEXT.chars().count())?);
        let fonts = crate::Fonts::new();
        let width = |size: Sp, spacing: Em| -> Result<f32, TestError> {
            let style = TextStyle::new("sans-serif", size, Color::WHITE)
                .letter_spacing(spacing)
                .resolve_unscaled();
            let collection = fonts.collection()?;
            let layout = Layout::new(&style, 1000.dp())?;
            Ok(build(
                &fonts,
                collection,
                crate::text::TextRendering::default(),
                TEXT,
                &style,
                &layout,
            )
            .max_intrinsic_width())
        };
        for size in [20.sp(), 40.sp()] {
            let spaced = width(size, 0.1.em())?;
            let spacing = spaced - width(size, Em::ZERO)?;
            let expected = glyphs * crate::scalar::scalar((size * 0.1).get())?;
            // Every glyph adds its advance and its spacing to the widths, each with at most one rounding at the
            // size of the spaced width.
            let rounding_size = 2.0 * glyphs * spaced;
            assert!(
                same_at(spacing, expected, rounding_size),
                "{size:?}: {spacing}"
            );
        }
        Ok(())
    }
}
