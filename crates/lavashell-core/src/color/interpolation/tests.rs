// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Easing, Keyframe, Keyframes, Motion, Playback, Tween};
use crate::color::samples::*;
use crate::float_asserts::assert_f64_eq;
use crate::test_error::{TestError, TestResult};
use ColorInterpolation::{LinearRgb, Oklab, Oklch, Srgb};
use std::time::Duration;

#[test]
fn oklch_interpolates_lightness_chroma_and_hue_directly() {
    let middle = BLUE.mix_in(VIOLET, 0.5, Oklch(HuePath::Shorter));
    assert_halfway_from_blue_to_violet(middle, HuePath::Shorter);
}

#[test]
fn oklab_midpoint_loses_chroma_that_the_default_oklch_keeps() {
    let chroma = |color: Color| color.to_oklch()[1];
    let oklab = chroma(GREEN.mix_in(BLUE, 0.5, Oklab));
    let oklch = chroma(GREEN.mix_in(BLUE, 0.5, Oklch(HuePath::Shorter)));
    assert!(oklab < chroma(GREEN), "{oklab}");
    assert_f64_eq(oklch, f64::midpoint(chroma(GREEN), chroma(BLUE)));
    assert_f64_eq(GREEN.mix(BLUE, 0.5).to_oklch()[1], oklch);
}

#[test]
fn hue_paths_follow_css_color_4() {
    use HuePath::*;
    for (from, to, path, middle) in [
        (350.0, 10.0, Shorter, 0.0),
        (350.0, 10.0, Longer, 180.0),
        (350.0, 10.0, Increasing, 0.0),
        (350.0, 10.0, Decreasing, 180.0),
        (10.0, 350.0, Shorter, 0.0),
        (10.0, 350.0, Longer, 180.0),
        (10.0, 350.0, Increasing, 180.0),
        (10.0, 350.0, Decreasing, 0.0),
        (40.0, 40.0, Shorter, 40.0),
        (40.0, 40.0, Longer, 220.0),
    ] {
        let a = Color::oklch(0.5, 0.1, from);
        let b = Color::oklch(0.7, 0.1, to);
        let hue = a.mix_in(b, 0.5, Oklch(path)).to_oklch()[2];
        assert!(
            hue_distance(hue, middle) <= hue_spacing(),
            "{path:?} {from} {to}: {hue}"
        );
    }
}

#[test]
fn an_achromatic_endpoint_takes_the_hue_of_the_other_endpoint() {
    for grey in [
        Color::oklch(0.6, 0.0, 120.0),
        Color::rgb(0x0080_8080),
        Color::WHITE,
    ] {
        for (a, b) in [(grey, VIOLET), (VIOLET, grey)] {
            let hue = a.mix_in(b, 0.5, Oklch(HuePath::Shorter)).to_oklch()[2];
            let violet = VIOLET.to_oklch()[2];
            assert!(
                hue_distance(hue, violet) <= hue_spacing(),
                "{grey:?}: {hue}"
            );
        }
    }
}

#[test]
fn equal_hues_at_another_chroma_do_not_turn_on_increasing_or_decreasing() {
    for hue in (0..360_i16).map(f64::from) {
        let (a, b) = (Color::oklch(0.6, 0.15, hue), Color::oklch(0.7, 0.05, hue));
        for path in [HuePath::Increasing, HuePath::Decreasing] {
            assert_shown_alike(a.mix_in(b, 0.5, Oklch(path)), Color::oklch(0.65, 0.1, hue));
        }
    }
}

#[test]
fn a_retarget_to_the_same_hue_after_a_wrapped_animation_does_not_turn() -> TestResult {
    let linear = Tween::new(Duration::from_secs(1)).easing(Easing::Linear);
    for hue in (0..360_i16).map(f64::from) {
        for (path, start) in [(HuePath::Increasing, -20.0), (HuePath::Decreasing, 20.0)] {
            let color =
                |lightness, hue| Color::oklch(lightness, 0.15, hue).interpolated(Oklch(path));
            let motion = Motion::default();
            let animation = motion.animation(color(0.6, (hue + start).rem_euclid(360.0)))?;
            animation.animate_to(color(0.6, hue), linear)?;
            motion.advance(Duration::from_secs(1));
            animation.animate_to(color(0.7, hue), linear)?;
            motion.advance(Duration::from_millis(1500));
            assert_shown_alike(animation.get().color, Color::oklch(0.65, 0.15, hue));
        }
    }
    Ok(())
}

fn hues_across_a_switch_from_oklab(
    start: Color,
    target: Color,
    path: HuePath,
) -> Result<Vec<f64>, TestError> {
    let linear = Tween::new(Duration::from_secs(1)).easing(Easing::Linear);
    let motion = Motion::default();
    let animation = motion.animation(start.interpolated(Oklab))?;
    animation.animate_to(target.interpolated(Oklab), linear)?;
    motion.advance(Duration::from_millis(500));
    let hue = || animation.get().color.to_oklch()[2];
    let mut hues = vec![hue()];
    animation.animate_to(target.interpolated(Oklch(path)), linear)?;
    for milliseconds in [750, 1000, 1250, 1500] {
        motion.advance(Duration::from_millis(milliseconds));
        hues.push(hue());
    }
    Ok(hues)
}

#[test]
fn a_color_at_rest_keeps_its_hue_when_only_its_interpolation_changes_from_oklab() -> TestResult {
    for hue in (0..360_i16).map(f64::from) {
        let color = Color::oklch(0.6, 0.15, hue);
        let expected = color.to_oklch()[2];
        for path in [HuePath::Increasing, HuePath::Decreasing, HuePath::Longer] {
            for sample in hues_across_a_switch_from_oklab(color, color, path)? {
                assert!(
                    hue_distance(sample, expected) <= hue_spacing(),
                    "{path:?} {hue}: {sample}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_color_in_flight_takes_the_shorter_hue_path_when_only_its_interpolation_changes_from_oklab()
-> TestResult {
    for hue in (0..360_i16).map(f64::from) {
        for turn in [40.0, -40.0] {
            let start = Color::oklch(0.6, 0.15, hue);
            let target = Color::oklch(0.7, 0.15, (hue + turn).rem_euclid(360.0));
            let end = target.to_oklch()[2];
            for path in [HuePath::Increasing, HuePath::Decreasing, HuePath::Longer] {
                let distances: Vec<f64> = hues_across_a_switch_from_oklab(start, target, path)?
                    .into_iter()
                    .map(|sample| hue_distance(sample, end))
                    .collect();
                let context = format!("{path:?} {hue} {turn}: {distances:?}");
                assert!(distances[0] > hue_spacing(), "{context}");
                assert!(
                    distances
                        .windows(2)
                        .all(|pair| pair[1] <= pair[0] + hue_spacing()),
                    "{context}"
                );
                assert!(distances[4] <= hue_spacing(), "{context}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_retarget_to_the_last_keyframe_during_playback_ends_at_that_keyframe() -> TestResult {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, BLUE),
        Keyframe::new(Duration::from_secs(1), VIOLET),
    ])?;
    let motion = Motion::default();
    let animation = motion.animation(BLUE)?;
    animation.play(frames, Playback::default());
    motion.advance(Duration::from_millis(500));
    animation.animate_to(VIOLET, Tween::new(Duration::from_secs(1)))?;
    motion.advance(Duration::from_millis(1500));
    assert_eq!(animation.get().to_argb8(), VIOLET.to_argb8());
    Ok(())
}

#[test]
fn every_space_premultiplies_alpha_without_a_transparent_fringe() {
    for space in [Oklab, Oklch(HuePath::Shorter), LinearRgb, Srgb] {
        let middle = Color::TRANSPARENT.mix_in(Color::rgb(0x00ff_0000), 0.5, space);
        assert_eq!(middle.to_argb8(), 0x80ff_0000, "{space:?}");
    }
}

#[test]
fn rgb_spaces_interpolate_encoded_or_linear_channels() {
    let black = Color::rgb(0);
    let white = Color::rgb(0x00ff_ffff);
    assert_eq!(black.mix_in(white, 0.4, Srgb).to_argb8(), 0xff66_6666);
    assert_eq!(black.mix_in(white, 0.4, LinearRgb).to_argb8(), 0xffaa_aaaa);
}

/// The error to expect from the velocity computed by `converted_velocity` for channels near one. It is the error
/// of a central difference over the distance at which its truncation error and its rounding error are equal.
fn central_difference_error() -> f64 {
    f64::EPSILON.powf(2.0 / 3.0)
}

#[test]
fn the_velocity_of_a_color_follows_the_conversion_to_another_space() {
    let (lightness, chroma, turns) = (0.6, 0.1, 0.25);
    let (lightness_rate, chroma_rate, turns_rate) = (0.2, 0.01, 0.1);
    let at = |lightness, chroma, turns, alpha| Vector([lightness, chroma, turns, alpha]);
    let current = Trajectory {
        position: at(lightness, chroma, turns, 1.0),
        velocity: at(lightness_rate, chroma_rate, turns_rate, 0.0),
        end: at(lightness, chroma, turns, 1.0),
    };
    let velocity = Space::Oklab.converted(Space::Oklch, current).velocity.0;
    let (angle, angle_rate) = (
        turns * std::f64::consts::TAU,
        turns_rate * std::f64::consts::TAU,
    );
    let a_rate = chroma_rate * angle.cos() - chroma * angle.sin() * angle_rate;
    let b_rate = chroma_rate * angle.sin() + chroma * angle.cos() * angle_rate;
    for (converted, expected) in velocity
        .into_iter()
        .zip([lightness_rate, a_rate, b_rate, 0.0])
    {
        assert!(
            (converted - expected).abs() <= central_difference_error(),
            "{converted} != {expected}"
        );
    }
}

#[test]
fn a_grey_oklab_motion_has_the_cubic_slope_in_linear_rgb() {
    let current = Trajectory {
        position: Vector([0.5, 0.0, 0.0, 1.0]),
        velocity: Vector([0.1, 0.0, 0.0, 0.0]),
        end: Vector([0.5, 0.0, 0.0, 1.0]),
    };
    let velocity = Space::LinearRgb.converted(Space::Oklab, current).velocity.0;
    for channel in &velocity[..3] {
        assert!(
            (channel - 3.0 * 0.5_f64.powi(2) * 0.1).abs() <= central_difference_error(),
            "{channel}"
        );
    }
}

#[test]
fn a_color_at_rest_has_no_velocity_in_another_space() {
    let at_rest = Trajectory::at_rest(Vector([0.5, 0.1, 0.3, 1.0]));
    let velocity = Space::LinearRgb.converted(Space::Oklch, at_rest).velocity;
    assert_eq!(velocity, Vector([0.0; 4]));
}
