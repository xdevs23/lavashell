// SPDX-License-Identifier: GPL-3.0-only
use super::scene::affine;
use super::shape::{rect, rounded};
use super::{Check, Samples, TestError, TestResult, read};
use lavashell_core::animation::{
    Animatable, Direction, Easing, Keyframe, Keyframes, MotionSpec, Playback, Playing, Program,
    Repeat, Spring, Timing, Tween, Vector, VectorSpace,
};
use lavashell_core::geometry::{Axis, FinitePoint, Point, Rect};
use lavashell_core::scene::Affine;
use lavashell_core::scene::tree::{Clamp, Presence, PresenceMap, TransformMotion};
use lavashell_core::shape::{Corners, RoundedRect};
use lavashell_core::units::{Dp, DpExt, ScaleFactor};
use lavashell_protocol::encode;
use std::time::Duration;

/// The visible step of every sample motion toward a target.
const STEP: f64 = 0.5;

fn timing() -> Timing {
    Timing {
        start: Duration::from_millis(1500),
        paused: Some(Duration::from_millis(1700)),
        offset: Duration::from_millis(40),
        rate: 1.5,
    }
}

fn spring() -> Result<Spring, TestError> {
    Ok(Spring::new(Duration::from_millis(400), 0.8)?)
}

fn tween() -> Tween {
    Tween::new(Duration::from_millis(250))
        .delay(Duration::from_millis(50))
        .easing(Easing::QuartOut)
        .preserve_velocity(true)
}

/// A motion from `from` to `to` whose velocity differs from both.
fn playing<T: Animatable>(from: &T, to: T, spec: MotionSpec) -> Result<Playing<T>, TestError> {
    let (start, end) = (from.vector(), to.vector());
    let velocity = end.sub(&start).scale(0.5);
    let program = Program::target(start, end, velocity, spec, STEP)?;
    Ok(Playing::new(to, program, timing())?)
}

pub(super) fn opacity() -> Result<Playing<f64>, TestError> {
    playing(&0.25, 1.0, MotionSpec::Tween(tween()))
}

pub(super) fn clip() -> Result<Playing<RoundedRect>, TestError> {
    let to = RoundedRect::new(
        Rect::new(4.dp(), 8.dp(), 120.dp(), 60.dp()),
        Corners::all(10.dp()),
    );
    playing(&rounded(), to, MotionSpec::Spring(spring()?))
}

/// A presence map, which has no public constructor, read from its outer transform, stretch and surface.
fn presence_map() -> Result<PresenceMap, TestError> {
    let outer = Affine::scale(2.0, 0.5).translated(Point::new(3.dp(), 7.dp()));
    let stretch = (
        Point::new(1.dp(), 2.dp()),
        Point::new(4.dp(), 6.dp()),
        [0.5_f64, 2.0],
    );
    Ok(read(encode(&(outer, Some(stretch), rect()))?.bytes())?)
}

fn transform_motions() -> Result<Samples<TransformMotion>, TestError> {
    let clamp = Clamp {
        min: (-20).dp(),
        max: 80.dp(),
        axis: Axis::Vertical,
        scale: 0.5,
    };
    let faded = Presence::fade()
        .slide(FinitePoint::try_new(Point::new(0.dp(), 12.dp()))?)
        .scale(ScaleFactor::try_new(0.9)?);
    let presence = playing(
        &(Point::new(2.dp(), 3.dp()), faded),
        (Point::new(5.dp(), 7.dp()), Presence::default()),
        MotionSpec::Tween(tween()),
    )?;
    let transform = playing(&Affine::IDENTITY, affine(), MotionSpec::Spring(spring()?))?;
    let offset = playing(&0.dp(), 36.dp(), MotionSpec::Instant)?;
    let map = presence_map()?;
    Ok(every_variant! {
        TransformMotion::Affine(_) => TransformMotion::Affine(transform), [
            0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 224, 191, 0,
            0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0, 0,
            0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240,
            63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0,
            0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 224, 191, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0,
            8, 64, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 192, 63,
            0, 0, 0, 0, 0, 0, 208, 191, 0, 0, 0, 0, 0, 0, 224, 63, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0,
            0, 0, 0, 0, 12, 64, 2, 94, 56, 85, 41, 122, 106, 47, 64, 154, 153, 153, 153, 153, 153,
            233, 63, 0, 0, 0, 0, 0, 0, 224, 63, 1, 128, 202, 181, 238, 1, 1, 1, 128, 206, 228, 205,
            2, 0, 128, 180, 137, 19, 0, 0, 0, 0, 0, 0, 248, 63,
        ];
        TransformMotion::Clamp { .. } => TransformMotion::Clamp { offset, clamp }, [
            1, 0, 0, 0, 0, 0, 0, 66, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66, 64, 0, 0,
            0, 0, 0, 0, 50, 64, 0, 0, 0, 0, 0, 0, 0, 224, 63, 1, 128, 202, 181, 238, 1, 1, 1, 128,
            206, 228, 205, 2, 0, 128, 180, 137, 19, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 52,
            192, 0, 0, 0, 0, 0, 0, 84, 64, 1, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
        TransformMotion::Presence { .. } => TransformMotion::Presence { presence, map }, [
            2, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0,
            224, 63, 0, 0, 0, 0, 0, 0, 224, 63, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64,
            0, 0, 0, 0, 0, 0, 0, 0, 92, 65, 10, 33, 232, 248, 186, 191, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 40, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0,
            0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 224, 63, 92, 65,
            10, 33, 232, 248, 170, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 192, 1, 0, 128,
            229, 154, 119, 0, 128, 225, 235, 23, 1, 1, 0, 0, 0, 0, 0, 0, 224, 63, 1, 128, 202, 181,
            238, 1, 1, 1, 128, 206, 228, 205, 2, 0, 128, 180, 137, 19, 0, 0, 0, 0, 0, 0, 248, 63, 0,
            0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            224, 63, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28, 64, 1, 0, 0, 0, 0, 0, 0, 240,
            63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0,
            0, 0, 0, 0, 224, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0,
            0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64,
        ];
    })
}

fn programs() -> Result<Samples<Program<Dp>>, TestError> {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 0.dp()),
        Keyframe::new(Duration::from_millis(300), 24.dp())
            .easing(Easing::Smooth)
            .tangent(Vector([2.0])),
    ])?;
    let playback = Playback {
        repeat: Repeat::Count(3),
        direction: Direction::Alternate,
    };
    let spec = MotionSpec::Spring(spring()?);
    let target = Program::target(Vector([1.0]), Vector([24.0]), Vector([-3.0]), spec, STEP)?;
    Ok(every_variant! {
        Program::Target { .. } => target, [
            0, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 8, 192, 2, 94,
            56, 85, 41, 122, 106, 47, 64, 154, 153, 153, 153, 153, 153, 233, 63, 0, 0, 0, 0, 0, 0,
            224, 63,
        ];
        Program::Frames { .. } => Program::Frames { frames, playback }, [
            1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 128, 198, 134, 143, 1, 0, 0, 0, 0, 0, 0,
            56, 64, 2, 1, 0, 0, 0, 0, 0, 0, 0, 64, 1, 3, 1,
        ];
    })
}

fn motion_specs() -> Result<Samples<MotionSpec>, TestError> {
    Ok(every_variant! {
        MotionSpec::Instant => MotionSpec::Instant, [0];
        MotionSpec::Tween(_) => MotionSpec::Tween(tween()), [
            1, 0, 128, 229, 154, 119, 0, 128, 225, 235, 23, 1, 1,
        ];
        MotionSpec::Spring(_) => MotionSpec::Spring(spring()?), [
            2, 94, 56, 85, 41, 122, 106, 47, 64, 154, 153, 153, 153, 153, 153, 233, 63,
        ];
    })
}

fn easings() -> Result<Samples<Easing>, TestError> {
    Ok(every_variant! {
        Easing::Linear => Easing::Linear, [0]; Easing::QuartOut => Easing::QuartOut, [1];
        Easing::Smooth => Easing::Smooth, [2]; Easing::Smoother => Easing::Smoother, [3];
        Easing::CubicBezier(_) => Easing::cubic_bezier(0.25, 0.1, 0.75, 1.0)?, [
            4, 0, 0, 0, 0, 0, 0, 208, 63, 154, 153, 153, 153, 153, 153, 185, 63, 0, 0, 0, 0, 0, 0,
            232, 63, 0, 0, 0, 0, 0, 0, 240, 63,
        ];
    })
}

fn repeats() -> Samples<Repeat> {
    every_variant! {
        Repeat::Once => Repeat::Once, [0]; Repeat::Count(_) => Repeat::Count(3), [1, 3];
        Repeat::Forever => Repeat::Forever, [2];
    }
}

fn directions() -> Samples<Direction> {
    every_variant! {
        Direction::Forward => Direction::Forward, [0];
        Direction::Alternate => Direction::Alternate, [1];
    }
}

fn axes() -> Samples<Axis> {
    every_variant! {
        Axis::Horizontal => Axis::Horizontal, [0]; Axis::Vertical => Axis::Vertical, [1];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&transform_motions()?)?;
    check.samples(&programs()?)?;
    check.samples(&motion_specs()?)?;
    check.samples(&easings()?)?;
    check.samples(&repeats())?;
    check.samples(&directions())?;
    check.samples(&axes())
}
