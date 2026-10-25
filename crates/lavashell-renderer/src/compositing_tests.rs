// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_registry::QualityChange;
use crate::test_error::TestError;
use crate::test_scenes::{paint, paint_damage};
use crate::{BlendSpace, Error, Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{BudgetChooser, FrameBudget};
use std::time::Duration;

#[test]
fn blend_changes_rebuild_every_target_with_empty_or_partial_damage() -> Result<(), TestError> {
    let size = Rect::new(0.dp(), 0.dp(), 16.dp(), 8.dp());
    let fill = DrawCommand::Fill {
        shape: size.into(),
        brush: Color::WHITE.with_alpha(0.5).into(),
    };
    let tree = SceneTree::with_leaf([fill])?;
    let working = skia_safe::ImageInfo::new(
        (16, 8),
        skia_safe::ColorType::RGBAF16,
        skia_safe::AlphaType::Premul,
        crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
    );
    let output = skia_safe::ImageInfo::new(
        (16, 8),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let raster =
        || skia_safe::surfaces::raster(&working, None, None).ok_or(TestError::RasterSurface);
    let mut surfaces = [raster()?, raster()?, raster()?];
    let pixels = |surface: &mut skia_safe::Surface| {
        let mut data = vec![0_u8; 16 * 8 * 4];
        assert!(surface.read_pixels(&output, &mut data, 16 * 4, (0, 0)));
        data
    };
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for (index, surface) in surfaces.iter_mut().enumerate() {
        painter.effect_planner.surface = Some(SurfaceId(index as u64));
        paint_damage(
            &mut painter,
            surface.canvas(),
            &tree,
            Some(size),
            skia_safe::Color::BLACK,
        )?;
        assert!(
            pixels(surface)
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0].abs_diff(188) <= 1)
        );
    }
    for (space, expected) in [(BlendSpace::Srgb, 128_u8), (BlendSpace::Linear, 188_u8)] {
        painter.set_blend_space(space);
        for (index, surface) in surfaces.iter_mut().enumerate() {
            painter.effect_planner.surface = Some(SurfaceId(index as u64));
            let damage = (index == 1).then_some(Rect::new(0.dp(), 0.dp(), 2.dp(), 2.dp()));
            paint_damage(
                &mut painter,
                surface.canvas(),
                &tree,
                damage,
                skia_safe::Color::BLACK,
            )?;
            for pixel in pixels(surface).as_chunks::<4>().0 {
                assert!(
                    pixel[..3].iter().all(|value| value.abs_diff(expected) <= 1),
                    "surface {index} retained stale {space:?} coverage: {pixel:?}"
                );
                assert_eq!(pixel[3], 255);
            }
        }
        let damaged_before = painter.render_stats().damaged_pixels;
        painter.set_blend_space(space);
        for (index, surface) in surfaces.iter_mut().enumerate() {
            painter.effect_planner.surface = Some(SurfaceId(index as u64));
            paint_damage(
                &mut painter,
                surface.canvas(),
                &tree,
                None,
                skia_safe::Color::BLACK,
            )?;
        }
        assert_eq!(painter.render_stats().damaged_pixels, damaged_before);
    }
    Ok(())
}

#[test]
fn srgb_planned_backdrop_reads_current_layer_on_first_and_later_frames() -> Result<(), TestError> {
    assert_planned(false)?;
    assert_planned(true)
}

fn assert_planned(change_after_plan: bool) -> Result<(), TestError> {
    let mut planned = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    planned.set_blend_space(BlendSpace::Srgb);
    planned.effect_quality(BudgetChooser::default());
    let mut reference = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    reference.set_blend_space(BlendSpace::Srgb);
    for frame in 1..=3 {
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(Size::new(96.dp(), 48.dp()), Density::ONE));
        let mut drawing = Drawing::new(
            &mut commands,
            Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
            &context,
        );
        for x in 0..24_u16 {
            drawing.fill(
                Rect::new(4.dp() * f64::from(x), 0.dp(), 4.dp(), 48.dp()),
                if (u64::from(x) + frame) % 2 == 0 {
                    Color::WHITE
                } else {
                    Color::rgb(0x0000_00ff)
                },
            );
        }
        drawing.backdrop(
            Shape::rounded(Rect::new(8.dp(), 4.dp(), 80.dp(), 40.dp()), 4.dp()),
            constant!(Backdrop::blur(Dp::new(4.0))),
        );
        if change_after_plan {
            planned.set_blend_space(BlendSpace::Linear);
        }
        let tree = SceneTree::with_leaf(commands)?;
        assert_eq!(
            planned.plan_scene_effects(
                tree.at(Duration::ZERO),
                FrameBudget {
                    surface: SurfaceId(0),
                    gpu_time: Some(Duration::from_millis(20)),
                },
                frame,
                frame,
                Density::ONE,
            ),
            QualityChange::Unchanged
        );
        if change_after_plan {
            planned.set_blend_space(BlendSpace::Srgb);
        }
        let render = |painter: &mut Painter| -> Result<Vec<u8>, TestError> {
            let info = skia_safe::ImageInfo::new(
                (96, 48),
                skia_safe::ColorType::RGBAF16,
                skia_safe::AlphaType::Premul,
                crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
            );
            let mut surface =
                skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
            let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            paint(painter, surface.canvas(), &tree, Density::ONE);
            let info = skia_safe::ImageInfo::new(
                (96, 48),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                skia_safe::ColorSpace::new_srgb(),
            );
            let mut pixels = vec![0; 96 * 48 * 4];
            assert!(surface.read_pixels(&info, &mut pixels, 96 * 4, (0, 0)));
            Ok(pixels)
        };
        assert_eq!(
            render(&mut planned)?,
            render(&mut reference)?,
            "frame {frame}"
        );
    }
    Ok(())
}
