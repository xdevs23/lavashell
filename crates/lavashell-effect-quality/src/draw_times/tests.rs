// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{EffectDraw, EffectId, Planned, VariantOption};
use lavashell_core::scene::tree::SceneTree;

fn key(draw: u64, variant: u64) -> DrawTimeKey {
    DrawTimeKey {
        draw: draw_id(draw),
        variant: VariantId(variant),
    }
}

const SURFACE: SurfaceId = SurfaceId(0);

/// The id of effect draw number `ordinal` of effect 0 by `SceneTree::LEAF`, the leaf of a scene tree with one leaf, on
/// surface 0.
fn draw_id(ordinal: u64) -> DrawId {
    DrawId {
        surface: SurfaceId(0),
        leaf: SceneTree::LEAF,
        ordinal,
        effect: EffectId(0),
    }
}

fn record(times: &mut DrawTimes, key: DrawTimeKey, millis: &[u64]) {
    for millis in millis {
        times.record(SURFACE, key, Duration::from_millis(*millis));
    }
}

#[test]
fn one_slow_draw_leaves_the_expected_time_and_a_recurring_one_raises_it() {
    let mut times = DrawTimes::default();
    record(&mut times, key(1, 1), &[4]);
    assert_eq!(times.gpu_time(SURFACE, key(1, 1)).expected(), None);
    record(&mut times, key(1, 1), &[12, 3]);
    let drawn = times.gpu_time(SURFACE, key(1, 1)).drawn;
    assert_eq!(drawn.expected(), Some(Duration::from_millis(4)));
    assert_eq!(drawn.jitter(), Some(Duration::from_millis(1)));
    assert_eq!(drawn.samples(), 3);
    record(&mut times, key(1, 1), &[11]);
    assert_eq!(
        times.gpu_time(SURFACE, key(1, 1)).expected(),
        Some(Duration::from_millis(11))
    );
}

#[test]
fn slow_draws_stop_counting_once_newer_samples_fill_the_window() {
    let mut times = DrawTimes::default();
    record(&mut times, key(1, 1), &[12, 11, 2, 3]);
    assert_eq!(
        times.gpu_time(SURFACE, key(1, 1)).expected(),
        Some(Duration::from_millis(3))
    );
    record(&mut times, key(1, 1), &[2]);
    let drawn = times.gpu_time(SURFACE, key(1, 1)).drawn;
    assert_eq!(drawn.expected(), Some(Duration::from_millis(2)));
    assert_eq!(drawn.jitter(), Some(Duration::ZERO));
}

#[test]
fn the_first_measurement_stands_in_until_drawn_samples_recur() {
    let mut times = DrawTimes::default();
    assert!(times.needs_first_measurement(SURFACE, key(1, 1)));
    times.await_first_measurement(SURFACE, key(1, 1));
    assert!(!times.needs_first_measurement(SURFACE, key(1, 1)));
    assert_eq!(times.gpu_time(SURFACE, key(1, 1)).expected(), None);
    times.record_first_measurement(SURFACE, key(1, 1), Duration::from_millis(6));
    record(&mut times, key(1, 1), &[2]);
    assert_eq!(
        times.gpu_time(SURFACE, key(1, 1)).expected(),
        Some(Duration::from_millis(6))
    );
    record(&mut times, key(1, 1), &[2]);
    assert_eq!(
        times.gpu_time(SURFACE, key(1, 1)).expected(),
        Some(Duration::from_millis(2))
    );
    times.record_first_measurement(SURFACE, key(2, 1), Duration::from_millis(6));
    assert_eq!(times.gpu_time(SURFACE, key(2, 1)), GpuTime::default());
}

#[test]
fn an_unmeasurable_draw_waits_for_its_drawn_samples() {
    let mut times = DrawTimes::default();
    times.mark_unmeasurable(SURFACE, key(1, 1));
    assert!(!times.needs_first_measurement(SURFACE, key(1, 1)));
    times.record_first_measurement(SURFACE, key(1, 1), Duration::from_millis(6));
    assert_eq!(times.gpu_time(SURFACE, key(1, 1)).expected(), None);
    record(&mut times, key(1, 1), &[2, 2]);
    assert_eq!(
        times.gpu_time(SURFACE, key(1, 1)).expected(),
        Some(Duration::from_millis(2))
    );
}

#[test]
fn a_plan_keeps_the_estimates_of_its_draws_and_the_samples_of_its_choices() {
    let mut times = DrawTimes::default();
    for (draw, variant) in [(1, 1), (2, 1), (2, 2), (3, 1)] {
        times.await_first_measurement(SURFACE, key(draw, variant));
        times.record_first_measurement(SURFACE, key(draw, variant), Duration::from_millis(4));
        record(&mut times, key(draw, variant), &[7, 7]);
    }
    record(&mut times, key(1, 1), &[7]);
    times.record(SurfaceId(5), key(1, 1), Duration::from_millis(7));
    let draws = [2, 3].map(|draw| EffectDraw {
        id: draw_id(draw),
        frames_waited: 0,
        options: Vec::new(),
    });
    let chosen = VariantOption {
        variant: VariantId(2),
        gpu_time: GpuTime::default(),
    };
    let plan = FramePlan::for_draws(
        &draws,
        |draw| draw.id,
        |draw| {
            if draw.id == draw_id(2) {
                Planned::Draw(chosen)
            } else {
                Planned::Wait
            }
        },
    );
    times.retain_chosen(SURFACE, &plan);
    assert_eq!(times.gpu_time(SURFACE, key(1, 1)), GpuTime::default());
    times.record_first_measurement(SURFACE, key(2, 1), Duration::from_millis(4));
    let estimate = Some(Duration::from_millis(7));
    for unchosen in [key(2, 1), key(3, 1)] {
        let gpu_time = times.gpu_time(SURFACE, unchosen);
        assert_eq!((gpu_time.estimate, gpu_time.drawn.samples()), (estimate, 0));
    }
    assert_eq!(times.gpu_time(SURFACE, key(2, 2)).drawn.samples(), 2);
    assert_eq!(times.gpu_time(SurfaceId(5), key(1, 1)).drawn.samples(), 1);
    times.forget_surface(SurfaceId(5));
    assert_eq!(times.gpu_time(SurfaceId(5), key(1, 1)), GpuTime::default());
}
