// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{DrawTimeKey, DrawTimes, DrawnTimes};
use lavashell_core::scene::tree::SceneTree;

const FINE: VariantId = VariantId(1);

const COARSE: VariantId = VariantId(2);

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

fn millis(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn drawn(samples: &[u64]) -> DrawnTimes {
    let key = DrawTimeKey {
        draw: draw_id(1),
        variant: COARSE,
    };
    let mut times = DrawTimes::default();
    for sample in samples {
        times.record(SurfaceId(0), key, millis(*sample));
    }
    times.gpu_time(SurfaceId(0), key).drawn
}

/// An effect draw `id` of a backdrop blur that has waited `frames_waited` frames. Its variant `FINE` has a first
/// measurement of 4 ms and no drawn samples. Its variant `COARSE` has a first measurement of 1 ms and the drawn samples
/// `coarse`.
fn backdrop_blur(id: u64, frames_waited: u64, coarse: DrawnTimes) -> EffectDraw {
    let option = |variant, estimate, drawn| VariantOption {
        variant,
        gpu_time: GpuTime {
            estimate: Some(millis(estimate)),
            drawn,
        },
    };
    EffectDraw {
        id: draw_id(id),
        frames_waited,
        options: vec![
            option(FINE, 4, DrawnTimes::default()),
            option(COARSE, 1, coarse),
        ],
    }
}

fn frame(surface: SurfaceId, gpu_millis: u64) -> FrameBudget {
    FrameBudget {
        surface,
        gpu_time: Some(millis(gpu_millis)),
    }
}

fn chosen(chooser: &mut BudgetChooser, frame: FrameBudget, coarse: &[u64]) -> Option<Choice> {
    (chooser.choose(frame, &[backdrop_blur(1, 0, drawn(coarse))])).choice(draw_id(1))
}

#[test]
fn without_a_limit_the_best_variant_draws_however_long_it_takes() {
    let unlimited = FrameBudget {
        surface: SurfaceId(0),
        gpu_time: None,
    };
    assert_eq!(
        chosen(&mut BudgetChooser::default(), unlimited, &[9, 9, 9]),
        Some(Choice::Draw(FINE))
    );
}

#[test]
fn quality_drops_at_once_and_rises_once_the_chosen_variant_renewed_its_window() {
    let mut chooser = BudgetChooser::default();
    let surface = SurfaceId(0);
    assert_eq!(
        chosen(&mut chooser, frame(surface, 6), &[]),
        Some(Choice::Draw(FINE))
    );
    assert_eq!(
        chosen(&mut chooser, frame(surface, 1), &[]),
        Some(Choice::Draw(COARSE))
    );
    for samples in [&[][..], &[1], &[1, 1]] {
        assert_eq!(
            chosen(&mut chooser, frame(surface, 6), samples),
            Some(Choice::Draw(COARSE))
        );
    }
    assert_eq!(
        chosen(&mut chooser, frame(surface, 6), &[1, 1, 1]),
        Some(Choice::Draw(FINE))
    );
}

#[test]
fn a_raise_leaves_the_jitter_of_the_chosen_variant_spare_and_restarts_without_it() {
    let mut chooser = BudgetChooser::default();
    let surface = SurfaceId(0);
    assert_eq!(
        chosen(&mut chooser, frame(surface, 1), &[]),
        Some(Choice::Draw(COARSE))
    );
    // Each window of the repeating samples 2, 2 and 1 ms has an expected time of 2 ms and a jitter of 1 ms, half of the
    // expected time. The fine variant at 4 ms therefore needs 4 ms plus half of 4 ms, 6 ms in total.
    let mut pattern = [2, 2, 1].into_iter().cycle();
    let mut samples = pattern.by_ref().take(3).collect::<Vec<_>>();
    for gpu_millis in [6, 5, 6, 6, 6] {
        assert_eq!(
            chosen(&mut chooser, frame(surface, gpu_millis), &samples),
            Some(Choice::Draw(COARSE))
        );
        samples.extend(pattern.next());
    }
    assert_eq!(
        chosen(&mut chooser, frame(surface, 6), &samples),
        Some(Choice::Draw(FINE))
    );
}

#[test]
fn a_variant_dropped_for_what_its_draws_took_stays_dropped() {
    let mut chooser = BudgetChooser::default();
    let mut times = DrawTimes::default();
    let key = |variant| DrawTimeKey {
        draw: draw_id(1),
        variant,
    };
    let surface = SurfaceId(0);
    for (variant, blank_copy) in [(FINE, 4), (COARSE, 1)] {
        times.await_first_measurement(surface, key(variant));
        times.record_first_measurement(surface, key(variant), millis(blank_copy));
    }
    let mut choices = Vec::new();
    for _ in 0..8 {
        let draw = EffectDraw {
            id: draw_id(1),
            frames_waited: 0,
            options: [FINE, COARSE]
                .map(|variant| VariantOption {
                    variant,
                    gpu_time: times.gpu_time(surface, key(variant)),
                })
                .to_vec(),
        };
        let plan = chooser.choose(frame(surface, 6), &[draw]);
        times.retain_chosen(surface, &plan);
        let choice = plan.choice(draw_id(1));
        if let Some(Choice::Draw(variant)) = choice {
            let drawn = if variant == FINE { 7 } else { 1 };
            times.record(surface, key(variant), millis(drawn));
        }
        choices.push(choice);
    }
    let [fine, coarse] = [FINE, COARSE].map(|variant| Some(Choice::Draw(variant)));
    let settled = [fine, fine, coarse, coarse, coarse, coarse, coarse, coarse];
    assert_eq!(choices, settled);
}

#[test]
fn an_unmeasured_variant_fits_any_budget() {
    let mut chooser = BudgetChooser::default();
    let unmeasured = EffectDraw {
        id: draw_id(1),
        frames_waited: 0,
        options: [FINE, COARSE]
            .map(|variant| VariantOption {
                variant,
                gpu_time: GpuTime::default(),
            })
            .to_vec(),
    };
    let plan = chooser.choose(frame(SurfaceId(0), 0), &[unmeasured]);
    assert_eq!(plan.choice(draw_id(1)), Some(Choice::Draw(FINE)));
}

#[test]
fn waiting_draws_go_first_without_exceeding_the_gpu_time() {
    let mut chooser = BudgetChooser::default();
    let none = DrawnTimes::default();
    let plan = chooser.choose(
        frame(SurfaceId(0), 1),
        &[backdrop_blur(1, 0, none), backdrop_blur(2, 10, none)],
    );
    assert_eq!(plan.choice(draw_id(2)), Some(Choice::Draw(COARSE)));
    assert_eq!(plan.choice(draw_id(1)), Some(Choice::Wait));
}

#[test]
fn alternating_windows_keep_independent_quality_and_raises() {
    let mut chooser = BudgetChooser::default();
    assert_eq!(
        chosen(&mut chooser, frame(SurfaceId(11), 1), &[]),
        Some(Choice::Draw(COARSE))
    );
    let raise = [
        (&[][..], COARSE),
        (&[1], COARSE),
        (&[1, 1], COARSE),
        (&[1, 1, 1], FINE),
    ];
    for (samples, expected) in raise {
        assert_eq!(
            chosen(&mut chooser, frame(SurfaceId(21), 6), &[]),
            Some(Choice::Draw(FINE))
        );
        assert_eq!(
            chosen(&mut chooser, frame(SurfaceId(11), 6), samples),
            Some(Choice::Draw(expected))
        );
    }
    let plan = chooser.choose(frame(SurfaceId(21), 6), &[]);
    assert!(plan.choices().is_empty());
    assert_eq!(chooser.chosen.len(), 1);
    chooser.forget_surface(SurfaceId(11));
    assert!(chooser.chosen.is_empty());
}
