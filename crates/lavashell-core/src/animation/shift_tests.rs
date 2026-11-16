// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::test_error::TestResult;
use std::time::Duration;

fn linear() -> Tween {
    Tween::new(Duration::from_millis(1_000)).easing(Easing::Linear)
}

#[test]
fn shifting_a_running_motion_moves_its_value_and_end_but_keeps_its_timing() -> TestResult {
    let motion = Motion::default();
    let animation = motion.animation(0.0_f64)?;
    animation.animate_to(100.0, linear())?;
    motion.advance(Duration::from_millis(500));
    animation.shift_or_animate_to(150.0, linear())?;
    assert_f64_eq(animation.get(), 100.0);
    assert_eq!(motion.next_rest(), Some(Duration::from_millis(1_000)));
    motion.advance(Duration::from_millis(1_000));
    assert!(animation.is_finished());
    assert_f64_eq(animation.get(), 150.0);
    Ok(())
}

#[test]
fn shifting_an_animation_at_rest_starts_a_motion_to_the_target() -> TestResult {
    let motion = Motion::default();
    let animation = motion.animation(0.0_f64)?;
    animation.shift_or_animate_to(40.0, linear())?;
    assert!(animation.is_running());
    motion.advance(Duration::from_millis(500));
    assert_f64_eq(animation.get(), 20.0);
    Ok(())
}
