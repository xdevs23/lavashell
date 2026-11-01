// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::prelude::*;

fn render(
    painter: &mut crate::Painter,
    size: f64,
    target: TextDestination,
) -> Result<Vec<u8>, TestError> {
    let mut style = TextStyle::new("probe", Sp::new(size), Color::WHITE).resolve_unscaled();
    style.color = Color::WHITE;
    let commands = [DrawCommand::Text {
        text: "Mon 5 Oct e\u{301} office אבג".into(),
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
    let _chained_canvas = surface.canvas().clear(if target.opaque {
        skia_safe::Color::BLACK
    } else {
        skia_safe::Color::TRANSPARENT
    });
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
    let mut pixels = vec![0_u8; 1300 * 170 * 4];
    assert!(surface.read_pixels(&info, &mut pixels, 1300 * 4, (0, 0)));
    Ok(pixels)
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
    let value = "Mon 5 Oct e\u{301} office\nabc אבג";
    let mut style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
    style.max_lines = None;
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::None,
            ..Default::default()
        },
        target,
    );
    let first = painter.shaper.geometry(value, &style, 1200.dp());
    painter.set_text_rendering(
        TextRendering {
            hinting: Hinting::Full,
            ..Default::default()
        },
        target,
    );
    let other = painter.shaper.geometry(value, &style, 1200.dp());
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
    let colored = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] != p[1] || p[1] != p[2])
    };
    for geometry in [PixelGeometry::None, PixelGeometry::Rgb, PixelGeometry::Bgr] {
        for opaque in [false, true] {
            let target = TextDestination { opaque, geometry };
            let mut direct = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
            direct.set_text_rendering(request, target);
            assert_eq!(
                colored(&render(&mut direct, 13.0, target)?),
                opaque && geometry != PixelGeometry::None
            );
            let mut tiled = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?
                .with_raster_paths(true);
            tiled.set_text_rendering(request, target);
            assert_eq!(
                colored(&render(&mut tiled, 13.0, target)?),
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
            .with_raster_paths(true);
    let info = skia_safe::ImageInfo::new(
        (20, 20),
        skia_safe::ColorType::BGRA8888,
        skia_safe::AlphaType::Opaque,
        None,
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::BLUE);
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
    let read = info
        .with_dimensions((1, 1))
        .with_color_type(skia_safe::ColorType::RGBA8888);
    let mut pixel = [0_u8; 4];
    assert!(surface.read_pixels(&read, &mut pixel, 4, (12, 12)));
    assert_eq!(
        pixel,
        [0, 0, 255, 255],
        "the tile's empty corner replaced the target background"
    );
    Ok(())
}
