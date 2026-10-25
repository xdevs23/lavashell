// SPDX-License-Identifier: GPL-3.0-only
use super::tests::ms;
use super::*;
use crate::color::Color;
use crate::color::samples::assert_shown_alike;
use crate::float_asserts::closes_with_step;
use crate::test_error::{TestError, TestResult};
use std::time::Duration;

#[test]
fn colors_interpolate_premultiplied_without_transparent_fringe() {
    let midpoint = Color::WHITE.at(Color::rgb(0x00ff_ffff).vector().scale(0.5));
    assert_eq!(
        midpoint.to_argb8(),
        Color::rgba(0x00ff_ffff, 128).to_argb8()
    );
    let middle = Color::WHITE.at(Color::rgb(0x00ff_ffff)
        .vector()
        .add(&Color::rgb(0).vector())
        .scale(0.5));
    assert_eq!(middle.to_argb8(), Color::rgb(0x0063_6363).to_argb8());
}

#[test]
fn colors_animate_along_the_shorter_oklch_hue_path_and_settle_inside_one_turn() -> TestResult {
    use crate::color::samples::{hue_distance, hue_spacing};
    let motion = Motion::default();
    let a = motion.animation(Color::oklch(0.6, 0.15, 350.0))?;
    let target = Color::oklch(0.6, 0.15, 10.0);
    a.animate_to(target, Tween::new(ms(1000)).easing(Easing::Linear))?;
    motion.advance(ms(500));
    assert!(hue_distance(a.get().to_oklch()[2], 0.0) <= hue_spacing());
    motion.advance(ms(1000));
    assert_eq!(a.get(), target);
    Ok(())
}

#[test]
fn a_settled_color_is_its_target_as_given_in_the_same_model_and_channels() -> TestResult {
    use crate::color::{ColorInterpolation, Gamut};
    let start = Color::encoded([0.1, 0.2, 0.3], 0.0, Gamut::Srgb);
    let target = Color::encoded([166.0 / 255.0, 200.0 / 255.0, 1.0], 1.0, Gamut::Srgb);
    let physics = Spring::new(ms(450), 0.85)?;
    let settled = |run: &dyn Fn(&Animation<Color>, &Motion) -> TestResult| {
        let motion = Motion::default();
        let a = motion.animation(start)?;
        run(&a, &motion)?;
        assert!(a.is_finished());
        Ok::<_, TestError>(a.get())
    };
    let tween = settled(&|a, motion| {
        a.animate_to(target, Tween::new(ms(200)))?;
        motion.advance(ms(300));
        Ok(())
    })?;
    let spring = settled(&|a, motion| {
        a.animate_to(target, physics)?;
        motion.advance(ms(5000));
        Ok(())
    })?;
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, start),
        Keyframe::new(ms(200), target),
    ])?;
    let played = settled(&|a, motion| {
        a.play(frames.clone(), Playback::default());
        motion.advance(ms(300));
        Ok(())
    })?;
    let reduced = settled(&|a, motion| {
        a.animate_to(target, physics)?;
        motion.advance(ms(16));
        motion.reduced_motion(true);
        motion.advance(ms(32));
        Ok(())
    })?;
    for value in [tween, spring, played, reduced] {
        assert_eq!(value, target);
    }

    let target = target.interpolated(ColorInterpolation::Oklab);
    let motion = Motion::default();
    let a = motion.animation(start.interpolated(ColorInterpolation::Oklab))?;
    a.animate_to(target, Tween::new(ms(200)))?;
    motion.advance(ms(300));
    assert_eq!(a.get(), target);
    Ok(())
}

#[test]
fn retarget_into_another_space_continues_the_motion_of_the_current_color() -> TestResult {
    use crate::color::{ColorInterpolation, HuePath};
    let blue = Color::oklch(0.6, 0.15, 250.0);
    let red = Color::oklch(0.6, 0.15, 30.0);
    let spring = Spring::new(ms(450), 0.85)?;
    let motion = Motion::default();
    let [kept, changed] = [(); 2].map(|()| {
        let a = motion.animation(blue.interpolated(ColorInterpolation::Oklab))?;
        a.animate_to(red.interpolated(ColorInterpolation::Oklab), spring)?;
        Ok::<_, TestError>(a)
    });
    let (kept, changed) = (kept?, changed?);
    motion.advance(ms(100));
    changed.animate_to(
        red.interpolated(ColorInterpolation::Oklch(HuePath::Shorter)),
        spring,
    )?;
    motion.advance(ms(101));
    assert_shown_alike(changed.get().color, kept.get().color);
    Ok(())
}

#[test]
fn keyframes_in_different_spaces_play_each_segment_in_the_space_of_its_end() -> TestResult {
    use crate::color::samples::{hue_distance, hue_spacing};
    use crate::color::{ColorInterpolation, HuePath};
    let red = Color::rgb(0x00ff_0000);
    let blue = Color::rgb(0x0000_00ff);
    let frames = Keyframes::new([
        Keyframe::new(ms(0), red.interpolated(ColorInterpolation::Oklab)).easing(Easing::Linear),
        Keyframe::new(
            ms(1000),
            blue.interpolated(ColorInterpolation::Oklch(HuePath::Shorter)),
        ),
    ])?;
    let motion = Motion::default();
    let a = motion.animation(red.interpolated(ColorInterpolation::Oklab))?;
    a.play(frames, Playback::default());
    assert_eq!(a.get().color.to_argb8(), red.to_argb8());
    motion.advance(ms(500));
    let hue = |color: Color| color.to_oklch()[2];
    let turn = (hue(blue) - hue(red) + 180.0).rem_euclid(360.0) - 180.0;
    assert!(hue_distance(hue(a.get().color), hue(red) + turn / 2.0) <= hue_spacing());
    Ok(())
}

#[test]
fn a_keyframe_tangent_keeps_its_direction_in_the_space_of_the_segment_end() -> TestResult {
    use crate::color::{ColorInterpolation, HuePath};
    let tangent = [0.0, 1.0, -1.0, 0.0];
    let frames = Keyframes::new([
        Keyframe::new(
            ms(0),
            Color::rgb(0x00ff_0000).interpolated(ColorInterpolation::Oklab),
        )
        .tangent(Vector(tangent)),
        Keyframe::new(
            ms(1000),
            Color::rgb(0x0000_00ff).interpolated(ColorInterpolation::Oklch(HuePath::Shorter)),
        ),
    ])?;
    let (end, start) = frames.sample(Duration::ZERO);
    let oklab = |vector: Vector<4>| end.at(vector).color.to_oklab();
    let along = |time: f64| oklab(start.position.add(&start.velocity.scale(time)));
    // The slope of the color along the start velocity must equal the tangent. A central difference approaches the
    // slope with an error that shrinks with the square of its step.
    assert!(closes_with_step(2.0, |step| {
        let (ahead, behind) = (along(step), along(-step));
        ahead
            .into_iter()
            .zip(behind)
            .zip(tangent)
            .map(|((ahead, behind), tangent)| ((ahead - behind) / (2.0 * step) - tangent).abs())
            .fold(0.0, f64::max)
    }));
    Ok(())
}
