// SPDX-License-Identifier: GPL-3.0-only
use super::coordinate_fonts::CoordinateFonts;
use crate::failure::DrawFailure;
use crate::fonts::{LoadedFonts, has_weight_axis, skia_weight, with_weight_arguments};
use crate::paint::solid;
use crate::scalar::scalars;
use crate::text_cache::{CachedParagraph, ParagraphSource, TextCache};
use lavashell_core::prelude::*;
use skia_safe::font_style::{Slant, Weight, Width};
use skia_safe::{
    FontStyle, Paint,
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextStyle as SkTextStyle,
    },
};
use std::hash::{Hash, Hasher};
use std::ops::Range;

pub(super) struct Layout {
    size: f32,
    letter_spacing: f32,
    width: f32,
    foreground: Paint,
}

impl Layout {
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

    pub(super) fn cached<'c>(
        self,
        paragraphs: &'c mut TextCache,
        loaded_fonts: &LoadedFonts,
        coordinates: &mut CoordinateFonts,
    ) -> &'c mut CachedParagraph {
        let make = || {
            let fonts = ParagraphFonts {
                loaded_fonts,
                named_weights: self.fonts.clone(),
                coordinates,
            };
            let paragraph = build(fonts, self.rendering, self.text, self.style, &self.layout);
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

fn set_weight(
    text_style: &mut SkTextStyle,
    fonts: &mut FontCollection,
    families: &[&str],
    weight: FontWeight,
    slant: Slant,
) {
    let whole = skia_weight(weight);
    let font_style = FontStyle::new(whole, Width::NORMAL, slant);
    let _style_returned_for_chaining = text_style.set_font_style(font_style);
    let faces = fonts.find_typefaces(families, font_style);
    if (faces.first())
        .is_some_and(|face| has_weight_axis(face) && face.font_style().weight() != whole)
    {
        with_weight_arguments(whole, |arguments| {
            text_style.set_font_arguments(arguments);
        });
    } else {
        text_style.set_font_arguments(None);
    }
}

pub(super) struct ParagraphFonts<'f> {
    pub loaded_fonts: &'f LoadedFonts,
    pub named_weights: FontCollection,
    pub coordinates: &'f mut CoordinateFonts,
}

impl ParagraphFonts<'_> {
    fn paragraph_collection<'s>(
        self,
        whole: Weight,
        text_styles: impl IntoIterator<Item = &'s SkTextStyle>,
    ) -> FontCollection {
        let mut text_styles = text_styles.into_iter();
        if text_styles.any(|text_style| text_style.font_arguments().is_some()) {
            self.coordinates.collection(self.loaded_fonts, whole)
        } else {
            self.named_weights
        }
    }
}

struct StyledSpan {
    range: Range<usize>,
    style: SkTextStyle,
}

pub(super) fn build(
    mut fonts: ParagraphFonts<'_>,
    rendering: crate::text::TextRendering,
    text: &str,
    style: &TextStyle<Dp>,
    layout: &Layout,
) -> Paragraph {
    let mut ts = SkTextStyle::new();
    let families = fonts.loaded_fonts.families(&style.family);
    let _style_returned_for_chaining = ts
        .set_font_size(layout.size)
        .set_foreground_paint(&layout.foreground)
        .set_font_families(&families);
    let named_weights = &mut fonts.named_weights;
    set_weight(
        &mut ts,
        named_weights,
        &families,
        style.weight,
        Slant::Upright,
    );
    let _style_returned_for_chaining = ts.set_letter_spacing(layout.letter_spacing);
    let mut ps = ParagraphStyle::new();
    rendering.apply(&mut ts, &mut ps);
    crate::text_alignment::apply(style, &mut ps);
    let _paragraph_style_returned_for_chaining =
        ps.set_text_style(&ts).set_max_lines(style.max_lines);
    if let Some(ellipsis) = ellipsis(style) {
        let _paragraph_style_returned_for_chaining = ps.set_ellipsis(ellipsis);
    }
    let spans = span_styles(&mut fonts, text, style, &ts);
    let text_styles = std::iter::once(&ts).chain(spans.iter().map(|span| &span.style));
    let collection = fonts.paragraph_collection(skia_weight(style.weight), text_styles);
    let mut builder = ParagraphBuilder::new(&ps, collection);
    let mut cursor = 0;
    for span in &spans {
        let _builder_returned_for_chaining = builder.add_text(&text[cursor..span.range.start]);
        let _builder_returned_for_chaining = builder.push_style(&span.style);
        let _builder_returned_for_chaining = builder.add_text(&text[span.range.clone()]);
        let _builder_returned_for_chaining = builder.pop();
        cursor = span.range.end;
    }
    let _builder_returned_for_chaining = builder.add_text(&text[cursor..]);
    let mut paragraph = builder.build();
    paragraph.layout(layout.width);
    paragraph
}

fn span_styles(
    fonts: &mut ParagraphFonts<'_>,
    text: &str,
    style: &TextStyle<Dp>,
    paragraph_style: &SkTextStyle,
) -> Vec<StyledSpan> {
    let families = fonts.loaded_fonts.families(&style.family);
    let mut styles = Vec::new();
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
        let mut inline = paragraph_style.clone();
        let span_families = (span.family.as_ref()).map_or_else(
            || families.clone(),
            |family| fonts.loaded_fonts.families(family),
        );
        let _style_returned_for_chaining = inline.set_font_families(&span_families);
        let slant = if span.italic {
            Slant::Italic
        } else {
            Slant::Upright
        };
        let weight = span.weight.unwrap_or(style.weight);
        set_weight(
            &mut inline,
            &mut fonts.named_weights,
            &span_families,
            weight,
            slant,
        );
        styles.push(StyledSpan {
            range: span.range.clone(),
            style: inline,
        });
        cursor = span.range.end;
    }
    styles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float_asserts::same_at;
    use crate::test_error::TestError;
    use crate::test_font::{PROBE_FAMILY, probe_style_before_scaling};

    impl ParagraphInput<'_> {
        pub(crate) fn key(&self) -> u64 {
            self.key
        }
    }

    fn probe_paragraph(fonts: ParagraphFonts<'_>, weight: f64) -> Result<Paragraph, TestError> {
        let style = probe_style_before_scaling()
            .size(20.sp())
            .weight(FontWeight::try_new(weight)?)
            .resolve_unscaled();
        let layout = Layout::new(&style, 1000.dp())?;
        Ok(build(
            fonts,
            crate::text::TextRendering::default(),
            "Weight",
            &style,
            &layout,
        ))
    }

    fn painted_face_ids(paragraph: &Paragraph) -> Vec<u32> {
        (paragraph.get_fonts().iter())
            .map(|info| info.font.typeface().unique_id())
            .collect()
    }

    #[test]
    fn letter_spacing_in_em_adds_the_fraction_of_the_font_size_to_every_glyph()
    -> Result<(), TestError> {
        const TEXT: &str = "abc";
        let glyphs = f32::from(u16::try_from(TEXT.chars().count())?);
        let fonts = crate::Fonts::new().load()?;
        let mut coordinates = CoordinateFonts::default();
        let mut width = |size: Sp, spacing: Em| -> Result<f32, TestError> {
            let style = TextStyle::new("sans-serif", size, Color::WHITE)
                .letter_spacing(spacing)
                .resolve_unscaled();
            let paragraph_fonts = ParagraphFonts {
                loaded_fonts: &fonts,
                named_weights: fonts.collection(),
                coordinates: &mut coordinates,
            };
            let layout = Layout::new(&style, 1000.dp())?;
            Ok(build(
                paragraph_fonts,
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
            let rounding_size = 2.0 * glyphs * spaced;
            assert!(
                same_at(spacing, expected, rounding_size),
                "{size:?}: {spacing}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_weight_between_two_named_weights_renders_between_them_on_a_font_with_a_weight_axis()
    -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?.load()?;
        let mut coordinates = CoordinateFonts::default();
        let mut width = |weight: f64| -> Result<f32, TestError> {
            let paragraph_fonts = ParagraphFonts {
                loaded_fonts: &fonts,
                named_weights: fonts.collection(),
                coordinates: &mut coordinates,
            };
            Ok(probe_paragraph(paragraph_fonts, weight)?.max_intrinsic_width())
        };
        let widths = [width(500.0)?, width(550.0)?, width(600.0)?];
        assert!(widths[0] < widths[1] && widths[1] < widths[2], "{widths:?}");
        Ok(())
    }

    #[test]
    fn a_weight_between_two_named_weights_leaves_no_font_in_the_shared_collection()
    -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?.load()?;
        let mut shared = fonts.collection();
        let paragraph_fonts = ParagraphFonts {
            loaded_fonts: &fonts,
            named_weights: shared.clone(),
            coordinates: &mut CoordinateFonts::default(),
        };
        let painted = painted_face_ids(&probe_paragraph(paragraph_fonts, 550.0)?);
        let mut coordinate = SkTextStyle::new();
        let weight = FontWeight::try_new(550.0)?;
        set_weight(
            &mut coordinate,
            &mut shared,
            &[PROBE_FAMILY],
            weight,
            Slant::Upright,
        );
        let arguments = coordinate.font_arguments();
        assert!(arguments.is_some());
        let shared_faces = shared.find_typefaces_with_font_arguments(
            &[PROBE_FAMILY],
            coordinate.font_style(),
            arguments,
        );
        assert_eq!((painted.len(), shared_faces.len()), (1, 1));
        assert_ne!(painted[0], shared_faces[0].unique_id());
        Ok(())
    }

    #[test]
    fn paragraphs_at_one_weight_between_two_named_weights_paint_with_one_font()
    -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?.load()?;
        let mut coordinates = CoordinateFonts::default();
        let mut painted = || -> Result<Vec<u32>, TestError> {
            let paragraph_fonts = ParagraphFonts {
                loaded_fonts: &fonts,
                named_weights: fonts.collection(),
                coordinates: &mut coordinates,
            };
            Ok(painted_face_ids(&probe_paragraph(paragraph_fonts, 550.0)?))
        };
        let first = painted()?;
        assert_eq!(first.len(), 1);
        assert_eq!(first, painted()?);
        Ok(())
    }

    #[test]
    fn a_named_weight_on_a_font_with_a_weight_axis_paints_the_face_of_the_shared_collection()
    -> Result<(), TestError> {
        let fonts = crate::test_font::test_font()?.load()?;
        let mut shared = fonts.collection();
        let mut named = SkTextStyle::new();
        let weight = FontWeight::try_new(500.0)?;
        set_weight(
            &mut named,
            &mut shared,
            &[PROBE_FAMILY],
            weight,
            Slant::Upright,
        );
        assert!(named.font_arguments().is_none());
        let paragraph_fonts = ParagraphFonts {
            loaded_fonts: &fonts,
            named_weights: shared.clone(),
            coordinates: &mut CoordinateFonts::default(),
        };
        let painted = painted_face_ids(&probe_paragraph(paragraph_fonts, 500.0)?);
        let shared_faces: Vec<u32> = (shared
            .find_typefaces(&[PROBE_FAMILY], named.font_style())
            .iter())
        .map(skia_safe::Typeface::unique_id)
        .collect();
        assert_eq!(painted, shared_faces);
        Ok(())
    }
}
