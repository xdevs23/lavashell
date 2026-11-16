// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_registry::QualityChange;
use crate::test_error::TestError;
use crate::test_scenes::{TEST_SURFACE, paint, paint_damage};
use crate::test_surfaces::{
    EFFECT_STRIPES, EFFECT_SURFACE, Pixels, TestSurface, channel_differences, pixels,
};
use crate::{BlendSpace, Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{BudgetChooser, FrameBudget};
use std::time::Duration;

const BLEND_SURFACE: TestSurface = TestSurface::new(16, 8);

#[test]
fn blend_changes_rebuild_every_target_with_empty_or_partial_damage() -> Result<(), TestError> {
    let size = BLEND_SURFACE.bounds();
    let fill = DrawCommand::Fill {
        shape: size.into(),
        brush: Color::WHITE.with_alpha(0.5).into(),
    };
    let tree = SceneTree::with_leaf([fill])?.snapshot();
    let mut surfaces = [
        BLEND_SURFACE.raster()?,
        BLEND_SURFACE.raster()?,
        BLEND_SURFACE.raster()?,
    ];
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for (index, surface) in (0_u64..).zip(&mut surfaces) {
        paint_damage(
            &mut painter,
            SurfaceId::numbered(index),
            surface.canvas(),
            &tree,
            Some(size),
            skia_safe::Color::BLACK,
        )?;
        assert!(pixels(surface).iter().all(|p| p[0].abs_diff(188) <= 1));
    }
    for (space, expected) in [(BlendSpace::Srgb, 128_u8), (BlendSpace::Linear, 188_u8)] {
        painter.set_blend_space(space);
        for (index, surface) in (0_u64..).zip(&mut surfaces) {
            let damage = (index == 1).then_some(Rect::new(0.dp(), 0.dp(), 2.dp(), 2.dp()));
            paint_damage(
                &mut painter,
                SurfaceId::numbered(index),
                surface.canvas(),
                &tree,
                damage,
                skia_safe::Color::BLACK,
            )?;
            for pixel in &pixels(surface) {
                let [color @ .., alpha] = pixel;
                assert!(
                    channel_differences(color, &[expected; 3]).all(|difference| difference <= 1),
                    "surface {index} retained stale {space:?} coverage: {pixel:?}"
                );
                assert_eq!(*alpha, 255);
            }
        }
        let damaged_before = painter.render_stats().damaged_pixels;
        painter.set_blend_space(space);
        for (index, surface) in (0_u64..).zip(&mut surfaces) {
            paint_damage(
                &mut painter,
                SurfaceId::numbered(index),
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
        let context = DrawContext::new(Viewport::new(EFFECT_SURFACE.size(), Density::ONE));
        let mut drawing = Drawing::new(&mut commands, EFFECT_SURFACE.bounds(), &context);
        let (white, blue) = (Color::WHITE, Color::rgb(0x0000_00ff));
        EFFECT_STRIPES.fill_stripes(
            &mut drawing,
            if frame % 2 == 0 {
                [white, blue]
            } else {
                [blue, white]
            },
        );
        drawing.backdrop(
            Shape::rounded(Rect::new(8.dp(), 4.dp(), 80.dp(), 40.dp()), 4.dp()),
            constant!(Backdrop::blur(Dp::new(4.0))),
        );
        if change_after_plan {
            planned.set_blend_space(BlendSpace::Linear);
        }
        let tree = SceneTree::with_leaf(commands)?.snapshot();
        assert_eq!(
            planned.plan_scene_effects(
                tree.at(Duration::ZERO),
                FrameBudget {
                    surface: TEST_SURFACE,
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
        let render = |painter: &mut Painter| -> Result<Pixels<[u8; 4]>, TestError> {
            let mut surface = EFFECT_SURFACE.raster()?;
            paint(painter, surface.canvas(), &tree, Density::ONE);
            Ok(pixels(&mut surface))
        };
        assert_eq!(
            render(&mut planned)?,
            render(&mut reference)?,
            "frame {frame}"
        );
    }
    Ok(())
}
