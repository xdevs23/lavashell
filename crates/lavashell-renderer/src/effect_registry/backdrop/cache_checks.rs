// SPDX-License-Identifier: GPL-3.0-only
use super::glass::{BlurLevel, GlassPrograms};
use super::{Backend, CACHED, FULL_BLUR, ID, Shared};
use crate::effect_registry::{
    EffectBackend, EffectInput, EffectRequest, ShapePainter, SurfaceAccess,
};
use crate::failure::Failures;
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_STRIPES, Pixels, pixels};
use crate::{FieldRoom, UniformRange};
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::DrawId;
use skia_safe::{Canvas, Paint, Surface};
use std::{cell::RefCell, rc::Rc};

pub(super) fn draw_on(surface: SurfaceId) -> DrawId {
    DrawId {
        surface,
        leaf: SceneTree::LEAF,
        ordinal: 0,
        effect: ID,
    }
}

pub(super) fn background(canvas: &Canvas) -> Result<(), TestError> {
    let _canvas_returned_for_chaining = canvas.clear(skia_safe::Color::BLACK);
    let mut paint = Paint::default();
    let colors = [skia_safe::Color::WHITE, skia_safe::Color::BLUE];
    for (stripe, color) in EFFECT_STRIPES.stripes(colors) {
        let _paint_returned_for_chaining = paint.set_color(color);
        let _canvas_returned_for_chaining = canvas.draw_rect(crate::paint::skrect(stripe)?, &paint);
    }
    Ok(())
}

pub(super) fn setup() -> Result<(Backend, DrawCommand, EffectInput), TestError> {
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?;
    let shared = Rc::new(RefCell::new(Shared::new(glass)));
    Ok((
        Backend { shared },
        DrawCommand::Backdrop {
            shape: Shape::rounded(
                Rect::new(-3.25.dp(), 4.125.dp(), 63.5.dp(), 30.25.dp()),
                5.dp(),
            ),
            effect: Backdrop::glass(
                3.dp(),
                lavashell_core::effects::Glass {
                    refraction: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                    dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.5))),
                    rim: constant!(NonNegative::try_new(0.2)),
                },
            )?,
        },
        EffectInput {
            scene_generation: 1,
            cache_allowed: true,
        },
    ))
}

fn prepare(canvas: &Canvas, clip: bool) -> Result<(), TestError> {
    background(canvas)?;
    let _canvas_returned_for_chaining = canvas.scale((1.25, 1.25));
    if clip {
        let _canvas_returned_for_chaining = canvas.clip_rect(
            skia_safe::Rect::from_xywh(10.0, 10.0, 45.0, 20.0),
            None,
            false,
        );
    }
    Ok(())
}

struct ChannelMismatch {
    x: u16,
    y: u16,
    channel: usize,
    actual: u8,
    expected: u8,
}

struct PixelDifference {
    worst: u8,
    first_mismatches: Vec<ChannelMismatch>,
}

impl std::fmt::Display for ChannelMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "({}, {}) channel {}: {} against {}",
            self.x, self.y, self.channel, self.actual, self.expected
        )
    }
}

impl std::fmt::Display for PixelDifference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "largest channel difference {}", self.worst)?;
        self.first_mismatches
            .iter()
            .try_for_each(|mismatch| write!(formatter, "; {mismatch}"))
    }
}

impl PixelDifference {
    fn between(actual: &Pixels<[u8; 4]>, expected: &Pixels<[u8; 4]>) -> Result<Self, TestError> {
        let worst = actual.largest_channel_difference(expected)?;
        let first_mismatches = (actual.zipped(expected)?.positioned()?)
            .flat_map(|((x, y), &(actual, expected))| {
                (actual.iter().zip(expected).enumerate()).map(
                    move |(channel, (&actual, &expected))| ChannelMismatch {
                        x,
                        y,
                        channel,
                        actual,
                        expected,
                    },
                )
            })
            .filter(|mismatch| mismatch.actual.abs_diff(mismatch.expected) > 2)
            .take(12)
            .collect();
        Ok(Self {
            worst,
            first_mismatches,
        })
    }
}

pub fn assert_cached_glass_matches_ordinary_glass(
    surface: &mut Surface,
    reference: &mut Surface,
    clip: bool,
) -> Result<(), TestError> {
    let (resources, failures) = (
        PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        Failures::default(),
    );
    let (mut backend, command, input) = setup()?;
    let _restored_when_the_surface_drops = surface.canvas().save();
    prepare(surface.canvas(), clip)?;
    let request = |variant| EffectRequest {
        surface: SurfaceId::numbered(0),
        id: draw_on(SurfaceId::numbered(0)),
        variant,
        command: &command,
        input: Some(&input),
        access: SurfaceAccess::Cacheable,
        painter: ShapePainter::new(&resources, &failures),
    };
    backend.render(surface.canvas(), &request(FULL_BLUR));
    assert_eq!(backend.shared.borrow().entries.len(), 1);
    let captured = pixels(surface);
    prepare(reference.canvas(), clip)?;
    let DrawCommand::Backdrop { shape, effect } = &command else {
        return Err(TestError::Missing("backdrop command"));
    };
    GlassPrograms::new(&FieldRoom::new(UniformRange::VULKAN_REQUIRED)?)?.backdrop(
        reference.canvas(),
        &PaintResources::new(
            &FieldRoom::new(UniformRange::VULKAN_REQUIRED)?,
            Failures::default(),
        )?,
        shape,
        BlurLevel::new(effect, 0),
        SurfaceAccess::Current,
    )?;
    let expected = pixels(reference);
    let filled = PixelDifference::between(&captured, &expected)?;
    assert!(
        filled.worst <= 2,
        "cache fill differs from ordinary glass: {filled}"
    );
    let _canvas_returned_for_chaining = surface.canvas().restore_to_count(1);
    let _canvas_returned_for_chaining = surface.canvas().reset_matrix();
    prepare(surface.canvas(), clip)?;
    backend.render(surface.canvas(), &request(CACHED));
    let replayed = pixels(surface);
    let replay = PixelDifference::between(&captured, &replayed)?;
    assert!(replay.worst <= 2, "clip {clip}, replay differs: {replay}");
    Ok(())
}
