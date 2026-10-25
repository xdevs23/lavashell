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
