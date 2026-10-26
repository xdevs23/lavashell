// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{
    Direction, Easing, Keyframe, Keyframes, MotionSpec, Playback, Playing, Program, Repeat, Timing,
    Vector,
};
use crate::test_error::TestResult;
use std::time::Duration;

const STEP: f64 = 0.01;

const LATER: Timing = Timing {
    start: Duration::from_millis(250),
    paused: None,
    offset: Duration::ZERO,
    rate: 1.5,
};

#[test]
fn a_motion_reads_back_as_written() -> TestResult {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 0.0_f64),
        Keyframe::new(Duration::from_millis(300), 1.0).easing(Easing::EASE),
        Keyframe::new(Duration::from_millis(500), 0.25).tangent(Vector([2.0])),
    ])?;
    let playback = Playback {
        repeat: Repeat::Count(2),
        direction: Direction::Alternate,
    };
    let written = Playing::new(0.25, Program::Frames { frames, playback }, LATER)?;
    let bytes = postcard::to_allocvec(&written)?;
    let read: Playing<f64> = postcard::from_bytes(&bytes)?;
    assert!(read == written);
    assert_eq!(postcard::to_allocvec(&read)?, bytes);
    Ok(())
}

#[test]
fn a_timing_with_a_rate_of_zero_is_not_read() -> TestResult {
    let fields = (Duration::ZERO, None::<Duration>, Duration::ZERO, 0.0_f64);
    let bytes = postcard::to_allocvec(&fields)?;
    assert!(postcard::from_bytes::<Timing>(&bytes).is_err());
    Ok(())
}

#[test]
fn a_target_with_a_vector_that_is_not_finite_is_not_read() -> TestResult {
    let program: Program<f64> = Program::Target {
        from: Vector([f64::NAN]),
        to: Vector([1.0]),
        velocity: Vector([0.0]),
        spec: MotionSpec::Instant,
        step: STEP,
    };
    let bytes = postcard::to_allocvec(&program)?;
    assert!(postcard::from_bytes::<Program<f64>>(&bytes).is_err());
    Ok(())
}

#[test]
fn a_motion_with_an_end_that_is_not_finite_is_not_read() -> TestResult {
    let program = Program::<f64>::target(
        Vector([0.0]),
        Vector([1.0]),
        Vector([0.0]),
        MotionSpec::Instant,
        STEP,
    )?;
    let bytes = postcard::to_allocvec(&(f64::NAN, program, LATER))?;
    assert!(postcard::from_bytes::<Playing<f64>>(&bytes).is_err());
    Ok(())
}
