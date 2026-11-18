// SPDX-License-Identifier: GPL-3.0-only
use super::opacity_checks::{
    assert_first_frame_blur, assert_nested_groups, commands, faded, pixel,
};
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_surfaces::EFFECT_SURFACE;
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;

#[test]
fn backdrop_blurs_on_the_first_partial_opacity_frame_without_doubling_parent_alpha()
-> Result<(), TestError> {
    assert_first_frame_blur(&mut EFFECT_SURFACE.raster()?)
}

#[test]
fn nested_fading_retained_groups_preserve_hdr_and_blur_sampling_outside_panel_bounds()
-> Result<(), TestError> {
    assert_nested_groups(|| Ok(Box::new(EFFECT_SURFACE.raster()?)))
}

#[test]
fn independently_registered_destination_reader_blurs_through_opacity() -> Result<(), TestError> {
    assert_independent_reader(false)
}

#[test]
fn independently_registered_retained_reader_resamples_changing_parent() -> Result<(), TestError> {
    assert_independent_reader(true)
}

fn assert_independent_reader(retained: bool) -> Result<(), TestError> {
    use crate::effect_registry::{DrawTarget, EffectBackend, EffectRegistry, EffectRequest};
    use crate::test_variants::only_variant;
    use lavashell_effect_quality::{EffectId, OfferedVariant};
    use skia_safe::{Canvas, Paint, canvas::SaveLayerRec, image_filters};
    struct Reader;
    impl Reader {
        fn blur_backdrop(canvas: &Canvas) -> Result<(), DrawFailure> {
            let filter = image_filters::blur((5.0, 5.0), skia_safe::TileMode::Clamp, None, None)
                .ok_or(DrawFailure::ShaderRejected)?;
            let mut paint = Paint::default();
            let _paint_returned_for_chaining = paint.set_blend_mode(skia_safe::BlendMode::Src);
            let _restored_by_the_next_restore =
                canvas.save_layer(&SaveLayerRec::default().paint(&paint).backdrop(&filter));
            let _canvas_returned_for_chaining = canvas.restore();
            Ok(())
        }
    }
    impl EffectBackend for Reader {
        fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<OfferedVariant> {
            only_variant(1)
        }

        fn reads_destination(&self) -> bool {
            true
        }
        fn render(&mut self, canvas: &Canvas, request: &EffectRequest<'_>) {
            if let Err(failure) = Self::blur_backdrop(canvas) {
                request.painter.record(failure);
            }
        }
    }
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.replace_effect_registry(EffectRegistry::empty());
    let id = EffectId(0x1234);
    painter.effect_registry_mut().register_effect(
        id,
        |c| matches!(c, DrawCommand::Circle { .. }),
        Reader,
    )?;
    let mut commands = commands(0.5, 1.0);
    commands[25] = DrawCommand::Circle {
        center: EFFECT_SURFACE.center(),
        radius: 8.dp(),
        brush: Color::WHITE.into(),
    };
    let mut surface = EFFECT_SURFACE.raster()?;
    for pass in 0..4 {
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        if retained {
            let inside = commands[25..27].to_vec();
            let tree = faded(commands[..24].to_vec(), 0.5, inside)?;
            crate::test_scenes::paint(&mut painter, surface.canvas(), &tree, Density::ONE)?;
        } else {
            painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        }
        let expected = if pass < 3 { 0.25 } else { 0.75 };
        let actual = pixel(&mut surface, 38, 12)[0];
        assert!(
            (actual - expected).abs() < 0.005,
            "pass {pass}: {actual}, expected {expected}"
        );
        if pass == 2 {
            for command in &mut commands[..24] {
                if let DrawCommand::Fill { brush, .. } = command
                    && let Brush::Solid(color) = brush
                {
                    *color = if color.to_linear(Gamut::Srgb)[0] > 0.5 {
                        Color::rgb(0)
                    } else {
                        Color::WHITE
                    };
                }
            }
        }
    }
    assert_eq!(painter.render_stats().cache_bytes, 0);
    Ok(())
}
