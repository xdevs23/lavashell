// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{
    float_asserts::{assert_f64_eq, closes_with_step},
    geometry::*,
    output::finest_step,
    reactive::{Phase, Scope},
    test_error::{Required, TestError, TestResult},
    units::{Dp, DpExt},
};
use std::time::Duration;

pub(super) fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn absolute_spring_sampling_is_independent_of_frame_rate_and_skipped_frames() -> TestResult {
    for ratio in [0.15, 0.85, 1.0, 1.00001, 1.7, 8.0] {
        let spring = Spring::new(ms(460), ratio)?;
        let run = |times: &[u64]| -> Result<(f64, f64), TestError> {
            let motion = Motion::default();
            let a = motion.animation(0.0_f64)?;
            a.animate_to_with_velocity(720.0, Vector([93.0]), spring)?;
            for time in times {
                motion.advance(ms(*time));
            }
            Ok((a.get(), a.velocity().0[0]))
        };
        let fixed = run(&(0..=720).step_by(8).collect::<Vec<_>>())?;
        let jittered = run(&[0, 7, 14, 31, 36, 72, 90, 101, 440, 443, 598, 720])?;
        assert_f64_eq(fixed.0, jittered.0);
        assert_f64_eq(fixed.1, jittered.1);
    }
    Ok(())
}

#[test]
fn a_running_spring_ends_where_the_device_pixel_of_its_view_shows_no_change() -> TestResult {
    let spring = Spring::new(ms(400), 1.0)?;
    let rest = |pixel_at_start: Dp, pixel: Dp| -> Result<Duration, TestError> {
        let motion = Motion::default();
        let a = motion.animation(Dp::ZERO)?;
        motion.set_device_pixel(pixel_at_start);
        a.animate_to(100.dp(), spring)?;
        motion.set_device_pixel(pixel);
        motion.next_rest().required("rest time")
    };
    let (coarse, fine) = (1.dp(), 0.25.dp());
    assert_eq!(rest(coarse, fine)?, rest(fine, fine)?);
    assert!(rest(coarse, coarse)? < rest(fine, fine)?);
    Ok(())
}

#[test]
fn spring_and_tween_retarget_without_position_or_velocity_discontinuity() -> TestResult {
    for spec in [
        MotionSpec::Spring(Spring::new(ms(450), 0.85)?),
        Tween::new(ms(500))
            .easing(Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0)?)
            .into(),
    ] {
        let motion = Motion::default();
        let a = motion.animation(0.0_f64)?;
        a.animate_to(100.0, spec)?;
        motion.advance(ms(120));
        let value = a.get();
        let velocity = a.velocity().0[0];
        a.animate_to(-80.0, spec)?;
        motion.advance(ms(120));
        assert_f64_eq(a.get(), value);
        assert_f64_eq(a.velocity().0[0], velocity);
        motion.advance(ms(121));
        assert!(a.get().is_finite());
        motion.advance(ms(10_000));
        assert_f64_eq(a.get(), -80.0);
        assert!(!a.is_running());
    }
    Ok(())
}

#[test]
fn analytic_velocity_matches_the_derivative_in_all_damping_regimes() -> TestResult {
    for damping in [0.2, 0.9, 1.0, 1.00001, 2.0, 20.0] {
        let spring = Spring::new(ms(400), damping)?;
        let x = Vector([120.0]);
        let v = Vector([-37.0]);
        let time = 0.087;
        let (_, speed) = spring.sample(&x, &v, time);
        // A central difference approaches the derivative with an error that shrinks with the square of its step.
        // A wrong velocity leaves a gap of the size of its error.
        let closes = closes_with_step(2.0, |step| {
            let (before, _) = spring.sample(&x, &v, time - step);
            let (after, _) = spring.sample(&x, &v, time + step);
            ((after.0[0] - before.0[0]) / (2.0 * step) - speed.0[0]).abs()
        });
        assert!(closes, "{damping}");
    }
    Ok(())
}

#[test]
fn bezier_inversion_matches_known_curve_and_handles_flat_tangents() -> TestResult {
    let identity = Easing::cubic_bezier(0.0, 0.0, 1.0, 1.0)?;
    // The error of the inversion must be below half of the finest visible step, so no output shows it.
    for i in 0..=1000 {
        let x = f64::from(i) / 1000.0;
        assert!(
            (identity.sample(x).0 - x).abs() <= finest_step() / 2.0,
            "{x}"
        );
    }
    for curve in [
        Easing::EASE,
        Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0)?,
        Easing::Smooth,
    ] {
        for i in 1..1000 {
            let (value, slope) = curve.sample(f64::from(i) / 1000.0);
            assert!(value.is_finite() && slope.is_finite());
        }
    }
    assert!(Easing::cubic_bezier(-1.0, 0.0, 1.0, 1.0).is_err());
    Ok(())
}

fn frames() -> Result<Keyframes<f64>, KeyframesError> {
    Keyframes::new([Keyframe::new(ms(0), 0.0), Keyframe::new(ms(1000), 100.0)])
}

#[test]
fn pause_seek_resume_and_reverse_playback_keep_the_shared_clock_unchanged() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(0.0)?;
    a.play(
        frames()?,
        Playback {
            repeat: Repeat::Forever,
            direction: Direction::Alternate,
        },
    );
    motion.advance(ms(250));
    let first = a.get();
    a.pause();
    motion.advance(ms(700));
    assert_f64_eq(a.get(), first);
    assert!(!motion.is_active());
    a.seek(ms(750));
    assert_f64_eq(a.get(), 84.375);
    assert_eq!(motion.now(), ms(700));
    a.resume();
    motion.advance(ms(950));
    assert_f64_eq(a.get(), 100.0);
    motion.advance(ms(1450));
    assert_f64_eq(a.get(), 50.0);
    a.seek(ms(3000));
    assert_f64_eq(a.get(), 100.0);
    assert_eq!(motion.now(), ms(1450));
    motion.advance(ms(1700));
    assert_f64_eq(a.get(), 84.375);
    Ok(())
}

#[test]
fn timeline_completes_at_the_correct_alternating_endpoint_and_can_be_sought_again() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(0.0)?;
    a.play(
        frames()?,
        Playback {
            repeat: Repeat::Count(2),
            direction: Direction::Alternate,
        },
    );
    motion.advance(ms(2100));
    assert_f64_eq(a.get(), 0.0);
    assert_f64_eq(a.target(), 0.0);
    assert_eq!(a.elapsed(), ms(2000));
    assert!(!motion.is_active());
    a.seek(ms(500));
    assert_f64_eq(a.get(), 50.0);
    assert!(motion.is_active());
    Ok(())
}

#[test]
fn completed_playback_freezes_time_and_retargeting_uses_its_actual_endpoint() -> TestResult {
    let host = Motion::default();
    let a = host.animation(0.0_f64)?;
    a.play(
        frames()?,
        Playback {
            repeat: Repeat::Count(2),
            direction: Direction::Alternate,
        },
    );
    host.advance(ms(4000));
    assert_eq!(a.elapsed(), ms(2000));
    assert!(a.is_finished());
    a.animate_to(100.0, Tween::new(ms(500)))?;
    host.advance(ms(4250));
    assert_f64_eq(a.get(), 50.0);
    host.advance(ms(6000));
    assert_eq!(a.elapsed(), ms(500));
    a.seek(ms(250));
    assert_f64_eq(a.get(), 50.0);
    a.snap(25.0)?;
    assert_eq!(a.elapsed(), Duration::ZERO);
    a.seek(ms(250));
    assert_f64_eq(a.get(), 25.0);
    assert!(a.is_finished());
    Ok(())
}

#[test]
fn playback_status_invalidates_its_consumer_even_without_a_value_change() -> TestResult {
    let scope = Scope::default();
    let a = Animation::new(0.0_f64)?;
    assert!(scope.track(Phase::Composition, || a.is_finished()));
    a.animate_to(100.0, Spring::new(ms(450), 0.85)?)?;
    assert_f64_eq(a.get(), 0.0);
    assert!(scope.dirty().contains(Phase::Composition));
    assert!(scope.track(Phase::Composition, || a.is_running()));
    a.pause();
    assert!(!a.is_running());
    assert!(scope.dirty().contains(Phase::Composition));
    a.resume();
    assert!(a.is_running());
    Ok(())
}

#[test]
fn hermite_keyframes_preserve_explicit_tangents_at_internal_boundaries() -> TestResult {
    let frames: Keyframes<f64> = Keyframes::new([
        Keyframe::new(ms(0), 0.0).tangent(Vector([0.0])),
        Keyframe::new(ms(1000), 100.0).tangent(Vector([20.0])),
        Keyframe::new(ms(2000), 150.0).tangent(Vector([0.0])),
    ])?;
    let velocity = |nanos: u64| frames.sample(Duration::from_nanos(nanos)).1.velocity.0[0];
    assert_f64_eq(velocity(1_000_000_000), 20.0);
    // Across the keyframe at 1 s, the velocity changes no more than it changes within a nanosecond on either side.
    let (before, after) = (velocity(999_999_999), velocity(1_000_000_001));
    let within = (velocity(999_999_998) - before).abs() + (velocity(1_000_000_002) - after).abs();
    assert!((after - before).abs() <= within, "{before} and {after}");
    Ok(())
}

#[test]
fn reactive_sampling_invalidates_only_the_phase_that_read_the_animation() -> TestResult {
    let scope = Scope::default();
    let motion = Motion::default();
    let a = motion.animation(0.0_f64)?;
    assert_f64_eq(scope.track(Phase::Paint, || a.get()), 0.0);
    a.animate_to(1.0, Spring::new(ms(450), 0.85)?)?;
    motion.advance(ms(8));
    assert!(scope.dirty().contains(Phase::Paint));
    assert!(!scope.dirty().contains(Phase::Composition));
    assert!(!scope.dirty().contains(Phase::Layout));
    drop(a);
    motion.advance(ms(16));
    assert!(!motion.is_active());
    Ok(())
}

#[test]
fn tween_delay_zero_duration_and_cancel_have_defined_endpoints() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(0.0)?;
    a.animate_to(10.0, Tween::new(Duration::ZERO).delay(ms(50)))?;
    motion.advance(ms(49));
    assert_f64_eq(a.get(), 0.0);
    motion.advance(ms(50));
    assert_f64_eq(a.get(), 10.0);
    a.animate_to(100.0, Spring::new(ms(450), 0.85)?)?;
    motion.advance(ms(80));
    let value = a.get();
    a.stop();
    motion.advance(ms(1000));
    assert_f64_eq(a.get(), value);
    assert!(!a.is_running());
    Ok(())
}

#[test]
fn playback_rate_preserves_position_pause_and_world_velocity() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(0.0_f64)?;
    a.play(frames()?, Playback::default());
    motion.advance(ms(250));
    let position = a.get();
    let speed = a.velocity().0[0];
    a.playback_rate(2.0)?;
    assert_f64_eq(a.get(), position);
    assert_f64_eq(a.velocity().0[0], 2.0 * speed);
    motion.advance(ms(375));
    assert_f64_eq(a.get(), 50.0);
    a.pause();
    a.playback_rate(0.5)?;
    motion.advance(ms(900));
    assert_f64_eq(a.get(), 50.0);
    a.resume();
    motion.advance(ms(1400));
    assert_f64_eq(a.get(), 84.375);
    a.animate_to_with_velocity(0.0, Vector([37.0]), Spring::new(ms(450), 0.85)?)?;
    assert_f64_eq(a.velocity().0[0], 37.0);
    assert!(a.playback_rate(f64::NAN).is_err());
    Ok(())
}

#[test]
fn expired_host_can_rebind_without_losing_progress_or_pause() -> TestResult {
    let a = Animation::new(0.0_f64)?;
    {
        let host = Motion::default();
        host.bind(&a)?;
        a.play(frames()?, Playback::default());
        host.advance(ms(400));
        a.pause();
    }
    let position = a.get();
    let host = Motion::default();
    host.bind(&a)?;
    host.bind(&a)?;
    host.advance(ms(10));
    assert_f64_eq(a.get(), position);
    a.resume();
    host.advance(ms(110));
    assert_f64_eq(a.get(), 50.0);
    Ok(())
}

#[test]
fn independently_paced_hosts_cannot_compete_for_one_track() -> TestResult {
    let a = Animation::new(0.0_f64)?;
    let first = Motion::default();
    let second = Motion::default();
    first.bind(&a)?;
    assert_eq!(second.bind(&a), Err(BindError::OtherMotion));
    Ok(())
}

#[test]
fn tuples_and_arrays_keep_properties_on_one_atomic_timeline() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(([Point::new(0.dp(), 0.dp()); 2], 0.0_f64))?;
    a.animate_to(
        ([Point::new(100.dp(), 40.dp()); 2], 1.0),
        Tween::new(ms(1000)),
    )?;
    motion.advance(ms(500));
    assert_eq!(a.get(), ([Point::new(50.dp(), 20.dp()); 2], 0.5));
    Ok(())
}

#[test]
fn exact_easing_is_available_and_continuous_tweens_arrive_without_acceleration() -> TestResult {
    let duration = ms(1000);
    let started = duration / 4;
    let arrival = started + duration;
    let host = Motion::default();
    let exact = host.animation(0.0_f64)?;
    exact.animate_to(
        100.0,
        Tween::new(duration)
            .easing(Easing::Linear)
            .preserve_velocity(false),
    )?;
    host.advance(started);
    assert_f64_eq(exact.get(), 25.0);
    assert_f64_eq(exact.velocity().0[0], 100.0);
    let smooth = host.animation(0.0_f64)?;
    smooth.animate_to_with_velocity(100.0, Vector([30.0]), Tween::new(duration))?;
    // The velocity at the arrival is zero. The velocity at a time before the arrival, divided by that time,
    // therefore approaches the acceleration at the arrival. The quotient shrinks with the time only when the
    // motion arrives without acceleration.
    let acceleration = |before: f64| {
        arrival
            .checked_sub(Duration::from_secs_f64(before))
            .map_or(f64::NAN, |at| {
                host.advance(at);
                smooth.velocity().0[0].abs() / before
            })
    };
    assert!(closes_with_step(1.0, acceleration));
    host.advance(arrival);
    assert_f64_eq(smooth.get(), 100.0);
    assert!(!smooth.is_running());
    Ok(())
}

#[test]
fn native_mutations_rebase_after_idle_without_requiring_timer_ticks() -> TestResult {
    let wall = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
    let clock = wall.clone();
    let host = Motion::default();
    host.time_source(move || clock.get());
    let a = host.animation(0.0_f64)?;
    wall.set(ms(10_000));
    a.animate_to(100.0, Tween::new(ms(1000)))?;
    host.advance(ms(10_100));
    let moving = a.get();
    assert!(moving > 0.0 && moving < 5.0);
    wall.set(ms(10_100));
    a.pause();
    wall.set(ms(30_000));
    a.resume();
    host.advance(ms(30_000));
    assert_f64_eq(a.get(), moving);
    host.advance(ms(30_100));
    assert!(a.get() > moving && a.get() < 10.0);
    Ok(())
}

#[test]
fn a_scale_animated_past_its_range_is_the_nearest_end() -> TestResult {
    use crate::units::ScaleFactor;
    let range = ScaleFactor::RANGE;
    let (smallest, largest) = (
        ScaleFactor::try_new(*range.start())?,
        ScaleFactor::try_new(*range.end())?,
    );
    for scale in [smallest, largest] {
        for (overshoot, end) in [(-1000.0, smallest), (1000.0, largest)] {
            assert_eq!(scale.with_parts(scale.parts() + overshoot), end);
        }
        assert_eq!(scale.with_parts(f64::NAN), scale);
    }
    Ok(())
}

#[test]
fn a_part_that_appears_in_a_retargeted_motion_shows_its_faded_start_at_once() -> TestResult {
    use crate::effects::Glass;
    use crate::units::{NonNegative, NonNegativeDp};
    let glass = Glass {
        refraction: NonNegativeDp::try_new(12.dp())?,
        dispersion: NonNegativeDp::try_new(0.7.dp())?,
        rim: NonNegative::try_new(0.14)?,
    };
    let motion = Motion::default();
    let a = motion.animation(None)?;
    motion.advance(ms(0));
    a.animate_to(Some(glass), Tween::new(ms(1000)))?;
    assert_eq!(a.get(), Some(glass.faded()));
    Ok(())
}

#[test]
fn a_value_or_a_velocity_that_is_not_finite_is_refused() -> TestResult {
    assert_eq!(Animation::new(f64::NAN).err(), Some(AnimationError::Value));
    assert_eq!(
        Motion::default().animation(f64::INFINITY).err(),
        Some(AnimationError::Value)
    );
    let a = Animation::new(0.0_f64)?;
    let spec = MotionSpec::from(Tween::new(ms(100)));
    assert_eq!(
        a.animate_to(f64::INFINITY, spec),
        Err(AnimationError::Value)
    );
    assert_eq!(
        a.animate_to_with_velocity(1.0, Vector([f64::NAN]), spec),
        Err(AnimationError::Velocity)
    );
    assert_eq!(a.snap(f64::NAN), Err(AnimationError::Value));
    assert_f64_eq(a.target(), 0.0);
    assert!(a.is_finished());
    Ok(())
}
