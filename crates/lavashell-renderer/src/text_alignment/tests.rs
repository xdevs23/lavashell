// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::prelude::*;

fn new_painter() -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::test_font()?;
    let painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    Ok(painter)
}

fn pixels(
    painter: &mut crate::Painter,
    style: &TextStyle<Dp>,
    text: &str,
) -> Result<Vec<u8>, TestError> {
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((300, 160)).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    painter.paint_as_leaf(
        surface.canvas(),
        &[DrawCommand::Text {
            text: text.into(),
            rect: Rect::new(0.dp(), 0.dp(), 296.dp(), 160.dp()),
            style: style.clone(),
        }],
        Density::ONE,
    )?;
    let info = surface.image_info();
    let mut result = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut result, info.min_row_bytes(), (0, 0)));
    Ok(result)
}

#[test]
fn neutral_alignment_preserves_the_backend_paragraph_defaults() {
    let native = ParagraphStyle::new();
    let mut configured = ParagraphStyle::new();
    apply(
        &TextStyle::new("sans-serif", 14.sp(), Color::WHITE).resolve_unscaled(),
        &mut configured,
    );
    assert_eq!(configured.text_align(), native.text_align());
    assert_eq!(configured.text_direction(), native.text_direction());
}

#[test]
fn wrapping_alignment_updates_geometry_pixels_and_cached_paragraphs() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let text = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau.";
    let mut style = TextStyle::new("probe", 10.sp(), Color::WHITE).resolve_unscaled();
    style.max_lines = None;
    let start_geometry = painter.shaper.geometry(text, &style, 296.dp());
    assert!(start_geometry.lines.len() > 1);
    let start = pixels(&mut painter, &style, text)?;
    let size = painter.shaper.measure(text, &style, 296.dp());
    style.alignment = TextAlignment::Center;
    let centered = painter.shaper.geometry(text, &style, 296.dp());
    assert_eq!(centered.lines, start_geometry.lines);
    assert_eq!(painter.shaper.measure(text, &style, 296.dp()), size);
    for line in &centered.lines {
        let end = line.start + text[line.clone()].trim_end().len();
        let ranges = centered.selection(line.start..end);
        let left = ranges.iter().map(|r| r.x).fold(Dp::MAX, Dp::min);
        let right = ranges.iter().map(|r| r.x + r.width).fold(Dp::ZERO, Dp::max);
        assert!((left + right - 296.dp()).abs() < 2.dp(), "{ranges:?}");
    }
    assert_ne!(pixels(&mut painter, &style, text)?, start);
    assert!(
        centered.caret(centered.lines[1].start).rect.x
            > start_geometry.caret(start_geometry.lines[1].start).rect.x
    );
    style.alignment = TextAlignment::Start;
    assert_eq!(pixels(&mut painter, &style, text)?, start);
    Ok(())
}

#[test]
fn logical_start_follows_rtl_direction_and_reverses_without_stale_cache() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let mut style = TextStyle::new("probe", 16.sp(), Color::WHITE).resolve_unscaled();
    let text = "אבג 123";
    let ltr = painter.shaper.geometry(text, &style, 296.dp());
    let before = pixels(&mut painter, &style, text)?;
    style.direction = ParagraphDirection::RightToLeft;
    let rtl = painter.shaper.geometry(text, &style, 296.dp());
    let bounds = rtl.selection(0..text.len());
    let right = bounds.iter().map(|r| r.x + r.width).fold(Dp::ZERO, Dp::max);
    assert!((right - 296.dp()).abs() < 2.dp());
    assert_ne!(pixels(&mut painter, &style, text)?, before);
    style.alignment = TextAlignment::End;
    let end = painter.shaper.geometry(text, &style, 296.dp());
    assert!(
        end.selection(0..text.len())
            .iter()
            .map(|r| r.x)
            .fold(Dp::MAX, Dp::min)
            < 2.dp()
    );
    assert_eq!(rtl.size, ltr.size);
    style.direction = ParagraphDirection::LeftToRight;
    style.alignment = TextAlignment::Start;
    assert_eq!(pixels(&mut painter, &style, text)?, before);
    Ok(())
}
