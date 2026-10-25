// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{
    Animatable, Animation, Direction, Easing, Keyframe, Keyframes, Motion, MotionSpec, Playback,
    Playing, Program, Repeat, Spring, Timing, Tween, Vector,
};
use crate::float_asserts::same_f64;
use crate::geometry::{Axis, Point, Viewport};
use crate::test_error::{TestError, TestResult};
use crate::units::Dp;
use std::time::Duration;

const START: Duration = Duration::from_millis(1_000);

const FROM_START: Timing = Timing {
    start: START,
    paused: None,
    offset: Duration::ZERO,
    rate: 1.0,
};

fn playing_from_start(end: f64, program: Program<f64>) -> Playing<f64> {
    Playing::new(end, program, FROM_START)
}

fn from_start(end: f64, program: Program<f64>) -> Slot<f64> {
    Slot::Motion(Box::new(playing_from_start(end, program)))
}

fn assert_same_samples(motion: &Motion, animation: &Animation<f64>, slot: &Slot<f64>) {
    for millis in [0, 16, 40, 120, 333, 700, 1_250, 2_000, 6_000] {
        let now = START + Duration::from_millis(millis);
        motion.advance(now);
        let (slotted, current) = (slot.at(now), animation.get());
        assert!(
            same_f64(slotted, current),
            "{millis} ms after the start: {slotted} differs from {current}"
        );
    }
}

fn started() -> Result<(Motion, Animation<f64>), TestError> {
    let motion = Motion::default();
    let animation = motion.animation(0.0_f64)?;
    motion.advance(START);
    Ok((motion, animation))
}

#[test]
fn a_tween_in_a_slot_equals_the_samples_of_the_animation() -> TestResult {
    let (motion, animation) = started()?;
    let tween = Tween::new(Duration::from_millis(300))
        .delay(Duration::from_millis(30))
        .easing(Easing::QuartOut);
    animation.animate_to(1.0, tween)?;
    let program = Program::Target {
        from: Vector([0.0]),
        to: Vector([1.0]),
        velocity: Vector([0.0]),
        spec: MotionSpec::Tween(tween),
        step: f64::visible_step(Viewport::default().pixel()),
    };
    assert_same_samples(&motion, &animation, &from_start(1.0, program));
    Ok(())
}

#[test]
fn a_spring_with_a_starting_velocity_in_a_slot_equals_the_samples_of_the_animation() -> TestResult {
    let (motion, animation) = started()?;
    let spring = Spring::new(Duration::from_millis(400), 0.6)?;
    animation.animate_to_with_velocity(2.0, Vector([-3.0]), spring)?;
    let program = Program::Target {
        from: Vector([0.0]),
        to: Vector([2.0]),
        velocity: Vector([-3.0]),
        spec: MotionSpec::Spring(spring),
        step: f64::visible_step(Viewport::default().pixel()),
    };
    assert_same_samples(&motion, &animation, &from_start(2.0, program));
    Ok(())
}

#[test]
fn keyframes_played_back_in_a_slot_equal_the_samples_of_the_animation() -> TestResult {
    let (motion, animation) = started()?;
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 0.0_f64),
        Keyframe::new(Duration::from_millis(200), 1.0).easing(Easing::Linear),
        Keyframe::new(Duration::from_millis(500), 0.25).tangent(Vector([2.0])),
    ])?;
    let playback = Playback {
        repeat: Repeat::Count(3),
        direction: Direction::Alternate,
    };
    animation.play(frames.clone(), playback);
    let program = Program::Frames { frames, playback };
    assert_same_samples(&motion, &animation, &from_start(0.25, program));
    Ok(())
}

#[test]
fn the_next_change_of_a_tree_follows_the_motions_in_its_slots() -> TestResult {
    let tween = Program::Target {
        from: Vector([0.0]),
        to: Vector([1.0]),
        velocity: Vector([0.0]),
        spec: MotionSpec::Tween(Tween::new(Duration::from_millis(300))),
        step: f64::visible_step(Viewport::default().pixel()),
    };
    let fading = Group {
        opacity: from_start(1.0, tween.clone()),
        ..Group::default()
    };
    let mut tree = SceneTree::default();
    let insert = Change::Insert {
        id: SceneNodeId(1),
        parent: None,
        index: 0,
        node: Node::Group(fading),
    };
    let changes = vec![insert];
    tree.show(SceneUpdate { changes })?;
    let after = |millis| START + Duration::from_millis(millis);
    assert_eq!(tree.next_change(Duration::from_millis(400)), Some(START));
    assert_eq!(tree.next_change(after(100)), Some(after(100)));
    assert_eq!(tree.next_change(after(300)), None);
    let paused = Timing {
        paused: Some(after(100)),
        ..FROM_START
    };
    let pause = Change::SetSlot {
        id: SceneNodeId(1),
        slot: SlotChange::Opacity(Slot::Motion(Box::new(Playing::new(
            1.0,
            tween.clone(),
            paused,
        )))),
    };
    let changes = vec![pause];
    tree.show(SceneUpdate { changes })?;
    assert_eq!(tree.next_change(after(100)), None);
    let playing = Change::SetSlot {
        id: SceneNodeId(1),
        slot: SlotChange::Opacity(from_start(1.0, tween)),
    };
    let removed = Change::Remove { id: SceneNodeId(1) };
    for (change, next) in [(playing, Some(after(100))), (removed, None)] {
        let changes = vec![change];
        tree.show(SceneUpdate { changes })?;
        assert_eq!(tree.next_change(after(100)), next);
    }
    Ok(())
}

#[test]
fn a_delayed_tween_changes_first_when_its_delay_ends() {
    let tween = Tween::new(Duration::from_millis(300)).delay(Duration::from_millis(200));
    let program = Program::Target {
        from: Vector([0.0]),
        to: Vector([1.0]),
        velocity: Vector([0.0]),
        spec: MotionSpec::Tween(tween),
        step: f64::visible_step(Viewport::default().pixel()),
    };
    let delayed = playing_from_start(1.0, program);
    let after = |millis| START + Duration::from_millis(millis);
    assert_eq!(delayed.next_change(after(50)), Some(after(200)));
    assert_eq!(delayed.next_change(after(250)), Some(after(250)));
    assert_eq!(delayed.next_change(after(500)), None);
}

#[test]
fn a_clamp_with_crossed_or_missing_bounds_still_moves_the_group() {
    let crossed = Clamp {
        min: Dp::new(40.0),
        max: Dp::new(10.0),
        axis: Axis::Vertical,
        scale: -1.0,
    };
    assert_eq!(
        crossed.translation(Dp::new(25.0)),
        Point::new(Dp::ZERO, Dp::new(-10.0))
    );
    let missing = Clamp {
        min: Dp::new(f64::NAN),
        max: Dp::new(f64::NAN),
        ..crossed
    };
    assert_eq!(
        missing.translation(Dp::new(25.0)),
        Point::new(Dp::ZERO, Dp::new(-25.0))
    );
}
