// SPDX-License-Identifier: GPL-3.0-only
use super::super::{Hinting, PixelGeometry};
use super::tests::{new_painter, target};
use super::*;
use crate::test_error::TestError;
use skia_safe::textlayout::{Paragraph, ParagraphStyle, TextStyle as SkTextStyle};

#[test]
fn approximate_native_fallback_keeps_the_default_font_edging() {
    let mut style = SkTextStyle::new();
    let mut paragraph = ParagraphStyle::new();
    TextRendering {
        antialiasing: TextAntialiasing::SubpixelApproximate,
        ..Default::default()
    }
    .apply(&mut style, &mut paragraph);
    assert_eq!(style.font_edging(), skia_safe::font::Edging::AntiAlias);
    assert_eq!(style.font_hinting(), SkTextStyle::new().font_hinting());
    assert_eq!(style.subpixel(), SkTextStyle::new().subpixel());
}

/// The edging, the hinting and the subpixel positioning of the font of a run.
type FontPolicy = (skia_safe::font::Edging, skia_safe::FontHinting, bool);

fn font_policies(paragraph: &mut Paragraph) -> Vec<FontPolicy> {
    let mut policies = Vec::new();
    paragraph.visit(|_, run| {
        if let Some(run) = run {
            let font = run.font();
            policies.push((font.edging(), font.hinting(), font.is_subpixel()));
        }
    });
    assert_ne!(policies, Vec::<FontPolicy>::new());
    policies
}

fn assert_geometry(actual: &TextGeometry, expected: &TextGeometry) {
    assert_eq!(actual.size, expected.size);
    assert_eq!(actual.lines, expected.lines);
    assert_eq!(actual.word_starts, expected.word_starts);
    let carets = |geometry: &TextGeometry| {
        geometry
            .carets
            .iter()
            .map(|caret| (caret.index, caret.rect))
            .collect::<Vec<_>>()
    };
    let clusters = |geometry: &TextGeometry| {
        geometry
            .clusters
            .iter()
            .map(|cluster| {
                (
                    cluster.byte_range.clone(),
                    cluster.bounds,
                    cluster.direction,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(carets(actual), carets(expected));
    assert_eq!(clusters(actual), clusters(expected));
}

#[test]
fn source_native_caches_keep_mask_and_ordinary_font_policies_separate() -> Result<(), TestError> {
    let mut painter = new_painter(PixelGeometry::Rgb)?;
    let value = "Headphones p e\u{301} office אבג";
    let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    let ordinary_input = painter.paragraph_input(value, &style, 280.dp())?;
    let ordinary = ordinary_input.key();
    let policies = font_policies(
        &mut ordinary_input
            .cached(&mut painter.paragraphs, &painter.registry)
            .shaped
            .paragraph,
    );
    assert!(
        policies
            .iter()
            .all(|policy| policy.0 == skia_safe::font::Edging::AntiAlias)
    );
    let (mut surface, _) = target()?;
    let input = RasterInput {
        canvas: surface.canvas(),
        policy: painter.text_rendering(),
        destination: painter.text_configuration.peek().destination,
        opaque_scope: true,
        style: &style,
        foreground: crate::paint::color4f(style.color)?,
    };
    let source = ApproximateCoverage::new()?.source_rendering(&input);
    assert_eq!(source.antialiasing, TextAntialiasing::Subpixel);
    let count = painter.fonts.paragraph_cache_mut().count();
    for ((hinting, native_hinting, fractional_positioning), iteration) in [
        (Hinting::Slight, skia_safe::FontHinting::Slight, true),
        (Hinting::Full, skia_safe::FontHinting::Full, false),
        (Hinting::Slight, skia_safe::FontHinting::Slight, true),
    ]
    .into_iter()
    .zip(0_u8..)
    {
        let source = TextRendering {
            hinting,
            fractional_positioning,
            ..source
        };
        // Each iteration uses a different width, which misses the paragraph cache of the painter every time.
        // The shaping inputs are the same. The paragraph cache inside SkParagraph ignores the font policy in its key
        // and could return a paragraph shaped with an earlier policy.
        let source_input = painter.source_paragraph_input(
            value,
            &style,
            Dp::new(280.0 + f64::from(iteration)),
            source,
        )?;
        assert_ne!(source_input.key(), ordinary);
        let actual = font_policies(
            &mut source_input
                .cached(&mut painter.paragraphs, &painter.registry)
                .shaped
                .paragraph,
        );
        assert!(actual.iter().all(|policy| {
            *policy
                == (
                    skia_safe::font::Edging::SubpixelAntiAlias,
                    native_hinting,
                    fractional_positioning,
                )
        }));
        assert_eq!(painter.fonts.paragraph_cache_mut().count(), count);
        assert_eq!(
            font_policies(painter.paragraphs.existing(ordinary)?),
            policies
        );
    }
    Ok(())
}

#[test]
fn lcd_source_warmup_preserves_default_wrapping_and_shaped_carets() -> Result<(), TestError> {
    let mut painter = new_painter(PixelGeometry::Rgb)?;
    let mut reference = new_painter(PixelGeometry::None)?;
    for size in [11.0, 13.0, 16.0, 100.0] {
        let mut style = TextStyle::new("probe", Sp::new(size), Color::WHITE).resolve_unscaled();
        style.max_lines = None;
        let value = "Headphones p e\u{301} office\nabc אבג";
        for width in [280.dp(), 79.dp(), 280.dp()] {
            let expected = reference.geometry(value, &style, width);
            assert_geometry(&painter.geometry(value, &style, width), &expected);
            let paragraph = painter.prepare_source_paragraph(
                value,
                &style,
                width,
                TextRendering {
                    antialiasing: TextAntialiasing::Subpixel,
                    ..Default::default()
                },
            )?;
            assert!(
                font_policies(paragraph)
                    .iter()
                    .all(|policy| { policy.0 == skia_safe::font::Edging::SubpixelAntiAlias })
            );
            assert_geometry(&painter.geometry(value, &style, width), &expected);
            assert_eq!(
                painter.measure(value, &style, width),
                reference.measure(value, &style, width)
            );
        }
    }
    Ok(())
}
