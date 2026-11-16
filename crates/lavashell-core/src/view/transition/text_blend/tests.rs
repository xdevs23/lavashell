// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Animation, Easing, Motion, Tween};
use crate::color::samples::BLUE;
use crate::float_asserts::assert_f64_eq;
use crate::test_error::{TestError, TestResult};
use crate::units::{EmExt, SpExt};
use std::time::Duration;

fn blend(weight: FontWeight, letter_spacing: Em) -> TextBlend {
    let style = TextStyle::new("sans-serif", 16.sp(), BLUE)
        .weight(weight)
        .letter_spacing(letter_spacing)
        .resolve_unscaled();
    TextBlend::of(&style, ColorInterpolation::default())
}

fn moving(
    from: TextBlend,
    to: TextBlend,
    easing: Easing,
) -> Result<(Animation<TextBlend>, Motion), TestError> {
    let motion = Motion::default();
    let animation = motion.animation(from)?;
    let second = Tween::new(Duration::from_secs(1))
        .easing(easing)
        .preserve_velocity(false);
    animation.animate_to(to, second)?;
    Ok((animation, motion))
}

#[test]
fn a_text_blend_moves_its_letter_spacing_halfway_in_half_the_time() -> TestResult {
    let (animation, motion) = moving(
        blend(FontWeight::NORMAL, Em::ZERO),
        blend(FontWeight::NORMAL, 0.1.em()),
        Easing::Linear,
    )?;
    motion.advance(Duration::from_millis(500));
    assert_f64_eq(animation.get().letter_spacing.get(), 0.05);
    Ok(())
}

#[test]
fn a_faded_text_blend_is_its_color_made_transparent() {
    let faded = blend(FontWeight::NORMAL, Em::ZERO).faded();
    assert_eq!(faded.color.color, BLUE.with_alpha(0.0));
}

#[test]
fn a_weight_that_swings_back_past_its_start_stops_at_the_heaviest_weight() -> TestResult {
    let anticipation = Easing::cubic_bezier(0.3, -0.6, 0.7, 0.4)?;
    let heaviest = FontWeight::try_new(1000.0)?;
    let (animation, motion) = moving(
        blend(heaviest, Em::ZERO),
        blend(FontWeight::BLACK, Em::ZERO),
        anticipation,
    )?;
    let weights: Vec<FontWeight> = (1..=10)
        .map(|tenth| {
            motion.advance(Duration::from_millis(100 * tenth));
            animation.get().weight
        })
        .collect();
    assert_eq!(weights[0], heaviest);
    assert!(weights.is_sorted_by(|a, b| a >= b), "{weights:?}");
    Ok(())
}
