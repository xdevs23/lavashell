// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_font::{probe_style, probe_style_at};
use crate::test_surfaces::{Pixels, has_colored_fringe, pixels, rgba8_pixel};
use crate::text::sample_fixtures::{mixed_script_label_text, multiline_label_text};
use lavashell_core::prelude::*;

fn render(
    painter: &mut crate::Painter,
    size: f64,
    target: TextDestination,
) -> Result<Pixels<[u8; 4]>, TestError> {
    let style = probe_style_at(Sp::new(size));
    let commands = [DrawCommand::Text {
        text: mixed_script_label_text(),
        rect: Rect::new(5.25.dp(), 4.5.dp(), 1200.dp(), 150.dp()),
        style,
    }];
    let alpha = if target.opaque {
        skia_safe::AlphaType::Opaque
    } else {
        skia_safe::AlphaType::Premul
    };
    let info = skia_safe::ImageInfo::new((1300, 170), skia_safe::ColorType::BGRA8888, alpha, None);
    let mut surface = skia_safe::surfaces::raster(&info, None, Some(&target.surface_props()))
        .ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(if target.opaque {
        skia_safe::Color::BLACK
    } else {
        skia_safe::Color::TRANSPARENT
    });
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
    Ok(pixels(&mut surface))
}

#[test]
fn policy_changes_replace_native_runs_and_keep_painters_independent() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut a = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let mut b = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let target = TextDestination::default();
    let none = TextRendering {
        hinting: Hinting::None,
        ..Default::default()
    };
    let full = TextRendering {
        hinting: Hinting::Full,
        ..Default::default()
    };
    a.set_text_rendering(none, target);
    b.set_text_rendering(full, target);
    let first = render(&mut a, 13.0, target)?;
    assert_eq!(b.shaper.skia_cache_count(), 0);
    let other = render(&mut b, 13.0, target)?;
    assert_ne!(
        first, other,
        "hinting changed neither raster coverage nor shaped geometry"
    );
    assert_eq!(render(&mut a, 13.0, target)?, first);
    assert_eq!(render(&mut b, 13.0, target)?, other);
    a.set_text_rendering(full, target);
    assert_eq!(a.shaper.skia_cache_count(), 0);
    assert_eq!(render(&mut a, 13.0, target)?, other);
    a.set_text_rendering(none, target);
    assert_eq!(render(&mut a, 13.0, target)?, first);
    assert_eq!(render(&mut b, 13.0, target)?, other);
    Ok(())
}

#[test]
fn fractional_positioning_changes_real_coverage_and_returns_to_cached_policy()
-> Result<(), TestError> {
    let mut painter = crate::Painter::new(
        &crate::test_font::test_font()?,
        crate::UniformRange::VULKAN_REQUIRED,
    )?;
    let target = TextDestination::default();
    let fractional = TextRendering {
        hinting: Hinting::None,
        ..Default::default()
    };
    let integral = TextRendering {
        fractional_positioning: false,
        ..fractional
    };
    painter.set_text_rendering(fractional, target);
    let first = render(&mut painter, 13.0, target)?;
    painter.set_text_rendering(integral, target);
    let other = render(&mut painter, 13.0, target)?;
    assert_ne!(first, other, "fractional glyph coverage remained unchanged");
    painter.set_text_rendering(fractional, target);
    assert_eq!(render(&mut painter, 13.0, target)?, first);
    Ok(())
}

#[test]
fn shaped_carets_follow_hinting_without_splitting_combining_or_bidi_clusters()
-> Result<(), TestError> {
    let mut painter = crate::Painter::new(
        &crate::test_font::test_font()?,
        crate::UniformRange::VULKAN_REQUIRED,
    )?;
    let target = TextDestination::default();
    let value = multiline_label_text();
    let mut style = probe_style();
    style.max_lines = None;
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::None,
            ..Default::default()
        },
        target,
    );
    let first = painter.shaper.geometry(&value, &style, 1200.dp());
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::Full,
            ..Default::default()
        },
        target,
    );
    let other = painter.shaper.geometry(&value, &style, 1200.dp());
    let bounds = |geometry: &TextGeometry| {
        geometry
            .clusters
            .iter()
            .map(|cluster| cluster.bounds)
            .collect::<Vec<_>>()
    };
    assert_ne!(
        bounds(&first),
        bounds(&other),
        "caret geometry ignored font hinting"
    );
    assert_eq!(first.lines, other.lines);
    assert_eq!(first.word_starts, other.word_starts);
    for geometry in [&first, &other] {
        let combined = value
            .find("e\u{301}")
            .ok_or(TestError::Missing("combining mark in the text"))?;
        assert!(
            !geometry
                .carets
                .iter()
                .any(|caret| caret.index == combined + 1)
        );
        assert_eq!(
            geometry
                .clusters
                .iter()
                .map(|c| &value[c.byte_range.clone()])
                .collect::<String>(),
            value,
        );
        assert!(geometry.clusters.iter().any(|cluster| {
            cluster.direction == lavashell_core::accessibility::TextDirection::RightToLeft
        }));
    }
    Ok(())
}

#[test]
fn lcd_requires_an_opaque_target_and_survives_the_tiled_backend() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let request = TextRendering {
        antialiasing: TextAntialiasing::Subpixel,
        ..Default::default()
    };
    for geometry in PixelGeometry::ALL {
        for opaque in [false, true] {
            let target = TextDestination { opaque, geometry };
            let mut direct = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
            direct.set_text_rendering(request, target);
            assert_eq!(
                has_colored_fringe(&render(&mut direct, 13.0, target)?),
                opaque && geometry != PixelGeometry::None
            );
            let mut tiled = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?
                .with_raster_mode(crate::raster::RasterMode::Tiles);
            tiled.set_text_rendering(request, target);
            assert_eq!(
                has_colored_fringe(&render(&mut tiled, 13.0, target)?),
                opaque && geometry != PixelGeometry::None
            );
        }
    }
    Ok(())
}

#[test]
fn cached_rgba_tiles_preserve_an_opaque_colored_background() -> Result<(), TestError> {
    let mut painter =
        crate::Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?
            .with_raster_mode(crate::raster::RasterMode::Tiles);
    let info = skia_safe::ImageInfo::new(
        (20, 20),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Opaque,
        None,
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::BLUE);
    let mut drawing = Vec::new();
    let context = DrawContext::new(Viewport::new(Size::new(10.dp(), 10.dp()), Density::ONE));
    Drawing::new(
        &mut drawing,
        Rect::new(4.dp(), 4.dp(), 10.dp(), 10.dp()),
        &context,
    )
    .path(
        &[
            Point::new(0.dp(), 0.dp()),
            Point::new(10.dp(), 0.dp()),
            Point::new(0.dp(), 10.dp()),
        ],
        true,
        Color::WHITE,
        None,
    );
    painter.paint_as_leaf(surface.canvas(), &drawing, Density::ONE)?;
    assert_eq!(
        rgba8_pixel(&mut surface, (12, 12), skia_safe::AlphaType::Opaque),
        [0, 0, 255, 255],
        "the tile's empty corner replaced the target background"
    );
    Ok(())
}
