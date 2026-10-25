// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effect_registry::{EffectBackend, EffectRegistry};
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_effect_quality::{BudgetChooser, GpuTime};
use std::time::Duration;

struct Blank;

impl EffectBackend for Blank {
    fn variants(&self, _: &DrawCommand, _: DrawTarget) -> Vec<VariantId> {
        vec![VariantId(1)]
    }

    fn render(&mut self, _: &Canvas, _: &EffectRequest<'_>) {}
}

const SURFACE: SurfaceId = SurfaceId(30);

const EFFECT: EffectId = EffectId(3000);

const REGULAR: Duration = Duration::from_millis(4);

const LIVE: DrawId = DrawId {
    surface: SURFACE,
    leaf: SceneTree::LEAF,
    ordinal: 0,
    effect: EFFECT,
};

fn sample(kind: SampleKind, elapsed: Duration) -> GpuSample {
    GpuSample {
        tag: SampleTag {
            kind,
            surface: SURFACE,
            frame: 1,
        },
        elapsed,
    }
}

fn key(draw: DrawId) -> DrawTimeKey {
    DrawTimeKey {
        draw,
        variant: VariantId(1),
    }
}

fn budget(gpu_time: Duration) -> FrameBudget {
    FrameBudget {
        surface: SURFACE,
        gpu_time: Some(gpu_time),
    }
}

fn circle() -> [DrawCommand; 1] {
    [DrawCommand::Circle {
        center: Point::new(10.dp(), 10.dp()),
        radius: 4.dp(),
        brush: Color::WHITE.into(),
    }]
}

/// A painter whose only effect replaces each circle with a draw of nothing, planned once for `tree` within 16 ms.
fn planned_painter(tree: &SceneTree) -> Result<Painter, TestError> {
    let mut painter = Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    painter.effect_registry = EffectRegistry::empty();
    painter.effect_registry_mut().register_effect(
        EFFECT,
        |command| matches!(command, DrawCommand::Circle { .. }),
        Blank,
    )?;
    painter.effect_quality(BudgetChooser::default());
    plan(&mut painter, tree, 1, budget(Duration::from_millis(16)));
    Ok(painter)
}

/// Plans `tree` as `frame` within `budget`. The only effect has one variant, so the choice never changes.
fn plan(painter: &mut Painter, tree: &SceneTree, frame: u64, budget: FrameBudget) {
    let at = tree.at(Duration::ZERO);
    let quality = painter.plan_scene_effects(at, budget, frame, 1, Density::ONE);
    assert_eq!(quality, QualityChange::Unchanged);
}

fn gpu_time(painter: &Painter, draw: DrawId) -> GpuTime {
    (painter.effect_planner.draw_times).gpu_time(SURFACE, key(draw))
}

fn timed(painter: &Painter) -> bool {
    painter.effect_planner.surfaces[&SURFACE].timed
}

#[test]
fn draw_times_follow_the_live_draws_and_surfaces() -> Result<(), TestError> {
    let tree = SceneTree::with_leaf(circle())?;
    let mut painter = planned_painter(&tree)?;
    let stale = DrawId { ordinal: 1, ..LIVE };
    for draw in [LIVE, LIVE, stale, stale] {
        let drawn = SampleKind::Effect(key(draw));
        painter
            .effect_planner
            .record_draw_time(sample(drawn, REGULAR));
    }
    plan(&mut painter, &tree, 2, budget(Duration::from_millis(16)));
    assert_eq!(gpu_time(&painter, LIVE).expected(), Some(REGULAR));
    assert_eq!(gpu_time(&painter, stale), GpuTime::default());
    painter.forget_effect_surface(SURFACE);
    let drawn = SampleKind::Effect(key(LIVE));
    painter
        .effect_planner
        .record_draw_time(sample(drawn, REGULAR));
    assert_eq!(gpu_time(&painter, LIVE), GpuTime::default());
    Ok(())
}

#[test]
fn a_first_measurement_arrives_for_the_variant_it_was_submitted_for() -> Result<(), TestError> {
    let tree = SceneTree::with_leaf(circle())?;
    let mut painter = planned_painter(&tree)?;
    (painter.effect_planner.draw_times).await_first_measurement(SURFACE, key(LIVE));
    let first = SampleKind::FirstEffectDraw(key(LIVE));
    painter
        .effect_planner
        .record_draw_time(sample(first, REGULAR));
    assert_eq!(gpu_time(&painter, LIVE).estimate, Some(REGULAR));
    assert_eq!(gpu_time(&painter, LIVE).drawn.samples(), 0);
    Ok(())
}

#[test]
fn frames_are_timed_until_samples_recur_and_then_while_the_plan_leaves_gpu_time()
-> Result<(), TestError> {
    let tree = SceneTree::with_leaf(circle())?;
    let mut painter = planned_painter(&tree)?;
    assert!(timed(&painter));
    for _ in 0..2 {
        let drawn = SampleKind::Effect(key(LIVE));
        painter
            .effect_planner
            .record_draw_time(sample(drawn, REGULAR));
    }
    plan(&mut painter, &tree, 2, budget(REGULAR * 2));
    assert!(timed(&painter));
    plan(&mut painter, &tree, 3, budget(REGULAR));
    assert!(!timed(&painter));
    Ok(())
}
