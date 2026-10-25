// SPDX-License-Identifier: GPL-3.0-only
use super::program::Scheduled;
use super::*;
use crate::float_asserts::{assert_f64_eq, same_f64};
use crate::geometry::{Point, Viewport};
use crate::reactive::{Phase, Scope};
use crate::scene::Affine;
use crate::test_error::{Required, TestResult};
use crate::units::{Dp, DpExt};
use std::rc::Rc;
use std::time::Duration;

fn started<T: Animatable>(value: T) -> Result<(Motion, Animation<T>), AnimationError> {
    let motion = Motion::default();
    let animation = motion.animation(value)?;
    motion.advance(Duration::from_millis(1_000));
    Ok((motion, animation))
}

#[test]
fn a_running_animation_comes_to_rest_at_the_time_its_clock_reports() -> TestResult {
    let specs = [
        MotionSpec::Tween(Tween::new(Duration::from_millis(300)).delay(Duration::from_millis(20))),
        MotionSpec::Spring(Spring::new(Duration::from_millis(400), 0.6)?),
        MotionSpec::Spring(Spring::new(Duration::from_millis(250), 1.0)?),
        MotionSpec::Spring(Spring::new(Duration::from_millis(500), 1.8)?),
    ];
    for spec in specs {
        let (motion, animation) = started(0.0_f64)?;
        animation.animate_to_with_velocity(40.0, Vector([-90.0]), spec)?;
        let rest = motion.next_rest().required("rest time")?;
        motion.advance(rest.saturating_sub(Duration::from_nanos(1)));
        assert!(animation.is_running(), "{spec:?}");
        motion.advance(rest);
        assert!(animation.is_finished(), "{spec:?}");
        assert!(animation.playing().is_none(), "{spec:?}");
        assert_eq!(motion.next_rest(), None, "{spec:?}");
    }
    Ok(())
}

#[test]
fn a_paused_or_endless_program_never_comes_to_rest() -> TestResult {
    let (motion, animation) = started(0.0_f64)?;
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 0.0),
        Keyframe::new(Duration::from_millis(200), 1.0),
    ])?;
    let forever = Playback {
        repeat: Repeat::Forever,
        direction: Direction::Alternate,
    };
    animation.play(frames, forever);
    assert!(animation.playing().is_some());
    assert_eq!(motion.next_rest(), None);
    animation.animate_to(5.0, Tween::new(Duration::from_millis(100)))?;
    animation.pause();
    assert!(animation.playing().is_none());
    assert_eq!(motion.next_rest(), None);
    Ok(())
}

#[test]
fn reading_the_program_follows_its_changes_and_not_the_moving_value() -> TestResult {
    let (motion, animation) = started(0.0_f64)?;
    let tween = Tween::new(Duration::from_millis(100));
    animation.animate_to(1.0, tween)?;
    let scope = Scope::default();
    let playing = || scope.track(Phase::Paint, || animation.playing().is_some());
    assert!(playing());
    motion.advance(Duration::from_millis(1_050));
    assert!(!scope.dirty().any());
    animation.animate_to(2.0, tween)?;
    assert!(scope.dirty().contains(Phase::Paint));
    assert!(playing());
    motion.advance(Duration::from_millis(1_200));
    assert!(scope.dirty().contains(Phase::Paint));
    Ok(())
}

#[test]
fn the_motion_read_while_it_plays_follows_each_change_of_its_timing() -> TestResult {
    let (motion, animation) = started(0.0_f64)?;
    animation.animate_to(1.0, Tween::new(Duration::from_millis(400)))?;
    let playing = || animation.playing().required("playing motion");
    let first = playing()?;
    motion.advance(Duration::from_millis(1_100));
    assert!(Rc::ptr_eq(&first, &playing()?));
    animation.playback_rate(2.0)?;
    let faster = playing()?;
    assert!(!Rc::ptr_eq(&first, &faster));
    animation.pause();
    assert!(animation.playing().is_none());
    motion.advance(Duration::from_millis(1_150));
    animation.resume();
    let resumed = playing()?;
    for millis in [1_150, 1_200, 1_260] {
        let now = Duration::from_millis(millis);
        motion.advance(now);
        let (played, current) = (resumed.at(now), animation.get());
        assert!(
            same_f64(played, current),
            "{millis} ms: {played} differs from {current}"
        );
    }
    Ok(())
}

#[test]
fn snapping_to_the_value_at_rest_leaves_readers_of_the_program_alone() -> TestResult {
    let (_motion, animation) = started(3.0_f64)?;
    let scope = Scope::default();
    let playing = || scope.track(Phase::Paint, || animation.playing().is_some());
    assert!(!playing());
    animation.snap(3.0)?;
    animation.animate_to(3.0, MotionSpec::Instant)?;
    assert!(!scope.dirty().any());
    animation.snap(4.0)?;
    assert!(scope.dirty().contains(Phase::Paint));
    assert!(!playing());
    animation.animate_to(9.0, Tween::new(Duration::from_millis(100)))?;
    assert!(playing());
    animation.snap(9.0)?;
    assert!(scope.dirty().contains(Phase::Paint));
    Ok(())
}

#[test]
fn the_clock_tells_read_motions_from_motions_that_only_come_to_rest() -> TestResult {
    let (motion, animation) = started(0.0_f64)?;
    let tween = Tween::new(Duration::from_millis(100));
    animation.animate_to(1.0, tween)?;
    let scope = Scope::default();
    assert!(scope.track(Phase::Paint, || animation.playing().is_some()));
    assert!(!motion.moves_read_values());
    assert_eq!(motion.next_rest(), Some(Duration::from_millis(1_100)));
    assert!(same_f64(scope.track(Phase::Paint, || animation.get()), 0.0));
    assert!(motion.moves_read_values());
    Ok(())
}

#[test]
fn a_motion_mapped_into_a_translation_moves_with_the_animation() -> TestResult {
    let translation = |vector: &Vector<2>| {
        let [x, y] = vector.0.map(Dp::new);
        Affine::translate(Point::new(x, y)).vector()
    };
    let check = |motion: &Motion, animation: &Animation<Point>| -> TestResult {
        let mapped = animation
            .playing()
            .required("playing motion")?
            .mapped(Affine::IDENTITY, translation);
        for millis in [1_000, 1_030, 1_100, 1_260, 1_500, 2_400] {
            let now = Duration::from_millis(millis);
            motion.advance(now);
            let moved = Affine::translate(animation.get()).translation;
            let played = mapped.at(now).translation;
            assert_f64_eq(played.x.get(), moved.x.get());
            assert_f64_eq(played.y.get(), moved.y.get());
        }
        Ok(())
    };
    let (motion, animation) = started(Point::new(0.dp(), 0.dp()))?;
    let spring = Spring::new(Duration::from_millis(400), 0.5)?;
    animation.animate_to_with_velocity(
        Point::new(30.dp(), -12.dp()),
        Vector([5.0, 60.0]),
        spring,
    )?;
    check(&motion, &animation)?;
    let (motion, animation) = started(Point::new(0.dp(), 0.dp()))?;
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, Point::new(0.dp(), 0.dp())),
        Keyframe::new(Duration::from_millis(300), Point::new(10.dp(), 4.dp()))
            .tangent(Vector([20.0, -8.0])),
        Keyframe::new(Duration::from_millis(600), Point::new(-6.dp(), 9.dp())),
    ])?;
    animation.play(frames, Playback::default());
    check(&motion, &animation)?;
    Ok(())
}

const FROM_ZERO: Timing = Timing {
    start: Duration::ZERO,
    paused: None,
    offset: Duration::ZERO,
    rate: 1.0,
};

fn pixel_step() -> f64 {
    f64::visible_step(Viewport::default().pixel())
}

#[test]
fn a_target_needs_finite_vectors_and_a_step_greater_than_zero() {
    let target = |from: f64, step: f64| {
        Program::<f64>::target(
            Vector([from]),
            Vector([1.0]),
            Vector([0.0]),
            MotionSpec::Instant,
            step,
        )
        .err()
    };
    for from in [f64::NAN, f64::INFINITY] {
        assert_eq!(target(from, pixel_step()), Some(MotionError::Vectors));
    }
    for step in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(target(0.0, step), Some(MotionError::Step));
    }
    assert_eq!(target(0.0, pixel_step()), None);
}

#[test]
fn a_motion_needs_a_finite_end() -> TestResult {
    let program = Program::target(
        Vector([0.0]),
        Vector([1.0]),
        Vector([0.0]),
        MotionSpec::Instant,
        pixel_step(),
    )?;
    let playing = Playing::new(f64::NAN, program, FROM_ZERO);
    assert_eq!(playing.err(), Some(MotionError::End));
    Ok(())
}

/// A motion from `-f64::MAX` toward `f64::MAX`, the whole range of f64.
fn across_the_range(spec: MotionSpec) -> Result<Scheduled<f64>, MotionError> {
    let program = Program::target(
        Vector([-f64::MAX]),
        Vector([f64::MAX]),
        Vector([0.0]),
        spec,
        pixel_step(),
    )?;
    Ok(Scheduled::new(program))
}

#[test]
fn a_tween_across_the_whole_range_passes_zero_halfway() -> TestResult {
    let tween = across_the_range(Tween::new(Duration::from_millis(100)).into())?;
    let at = |millis| tween.sample(Duration::from_millis(millis)).trajectory;
    let [early, halfway, late] = [25, 50, 75].map(|millis| at(millis).position.0[0]);
    assert!(halfway.abs() <= f64::MAX * f64::EPSILON, "{halfway}");
    assert!(-f64::MAX < early && early < halfway, "{early}");
    assert!(halfway < late && late < f64::MAX, "{late}");
    assert_eq!(at(50).velocity, Vector([f64::MAX]));
    Ok(())
}

#[test]
fn a_spring_across_the_whole_range_keeps_moving_toward_its_end() -> TestResult {
    let spring = across_the_range(Spring::new(Duration::from_millis(400), 1.0)?.into())?;
    let at = |millis| {
        spring
            .sample(Duration::from_millis(millis))
            .trajectory
            .position
            .0[0]
    };
    let (early, late) = (at(50), at(100));
    assert!(
        -f64::MAX < early && early < late && late < f64::MAX,
        "{early} {late}"
    );
    let rest = spring.rest().required("rest time")?;
    assert!(rest > Duration::from_millis(100));
    Ok(())
}

#[test]
fn a_keyframe_segment_across_the_whole_range_passes_zero_halfway() -> TestResult {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, -f64::MAX),
        Keyframe::new(Duration::from_millis(100), f64::MAX),
    ])?;
    let (frame, trajectory) = frames.sample(Duration::from_millis(50));
    assert_f64_eq(frame, f64::MAX);
    let position = trajectory.position.0[0];
    assert!(position.abs() <= f64::MAX * f64::EPSILON, "{position}");
    assert_eq!(trajectory.velocity, Vector([f64::MAX]));
    Ok(())
}
