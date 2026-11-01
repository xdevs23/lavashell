// SPDX-License-Identifier: GPL-3.0-only
use super::analysis::Analyses;
use crate::effect_registry::{DrawTarget, EffectBackend, EffectRegistry, EffectRequest};
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Packet;
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{EffectId, VariantId};
use skia_safe::Canvas;
use std::sync::Arc;

struct Idle;

impl EffectBackend for Idle {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        Vec::new()
    }

    fn render(&mut self, _: &Canvas, _: &EffectRequest<'_>) {}
}

fn circle() -> Packet {
    Packet::new(Arc::from([DrawCommand::Circle {
        center: Point::new(10.dp(), 10.dp()),
        radius: 4.dp(),
        brush: Color::WHITE.into(),
    }]))
}

fn layer_around_scaled_path(scale: f64) -> Packet {
    Packet::new(Arc::from([
        DrawCommand::OpacityLayer {
            alpha: 0.5,
            bounds: Rect::new(0.dp(), 0.dp(), 24.dp(), 24.dp()),
        },
        DrawCommand::Save,
        DrawCommand::Transform(Affine::scale(scale, scale)),
        DrawCommand::Path {
            start: Point::new(1.dp(), 1.dp()),
            segments: vec![Segment::Line(Point::new(9.dp(), 9.dp()))],
            closed: false,
            brush: Color::WHITE.into(),
            stroke: Some(1.dp()),
        },
        DrawCommand::Restore,
        DrawCommand::Restore,
    ]))
}

#[test]
fn a_layer_around_a_transform_expands_only_when_the_transformed_path_leaves_the_layer_bounds() {
    let (registry, mut analyses) = (EffectRegistry::empty(), Analyses::default());
    let inside = analyses.get(SurfaceId(1), &layer_around_scaled_path(2.0), &registry);
    assert!(!inside.expands_group(0));
    let past = analyses.get(SurfaceId(2), &layer_around_scaled_path(3.0), &registry);
    assert!(past.expands_group(0));
}

#[test]
fn an_effect_registered_after_an_analysis_is_seen_by_the_next_one() -> Result<(), TestError> {
    let (mut registry, mut analyses, packet) =
        (EffectRegistry::empty(), Analyses::default(), circle());
    assert!(!analyses.get(SurfaceId(1), &packet, &registry).has_effects());
    registry.register_effect(
        EffectId(912),
        |command| matches!(command, DrawCommand::Circle { .. }),
        Idle,
    )?;
    assert!(analyses.get(SurfaceId(1), &packet, &registry).has_effects());
    Ok(())
}

#[test]
fn a_new_registry_is_not_mistaken_for_the_one_before() -> Result<(), TestError> {
    let packet = circle();
    let mut analyses = Analyses::default();
    let mut registry = EffectRegistry::empty();
    registry.register_effect(
        EffectId(913),
        |command| matches!(command, DrawCommand::Circle { .. }),
        Idle,
    )?;
    assert!(analyses.get(SurfaceId(1), &packet, &registry).has_effects());
    assert!(
        !analyses
            .get(SurfaceId(1), &packet, &EffectRegistry::empty())
            .has_effects()
    );
    Ok(())
}
