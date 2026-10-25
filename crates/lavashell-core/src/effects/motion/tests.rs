// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Animation, Easing, Keyframe, Keyframes, Motion, Playback, Tween};
use crate::color::HuePath;
use crate::color::samples::{BLUE, GREEN, VIOLET, assert_halfway_from_blue_to_violet};
use crate::float_asserts::assert_f64_eq;
use crate::gradient::{ColorStop, Gradient, GradientKind};
use crate::scene::Brush;
use crate::test_error::{Required, TestError, TestResult};
use crate::units::{DpExt, NumberError};
use std::time::Duration;

fn linear_second() -> Tween {
    Tween::new(Duration::from_secs(1))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

fn moving(from: Material, to: Material) -> Result<(Animation<Material>, Motion), TestError> {
    let motion = Motion::default();
    let animation = motion.animation(from)?;
    animation.animate_to(to, linear_second())?;
    Ok((animation, motion))
}

fn at(animation: &Animation<Material>, motion: &Motion, milliseconds: u64) -> Material {
    motion.advance(Duration::from_millis(milliseconds));
    animation.get()
}

fn motion<const N: usize>(
    from: Material,
    to: Material,
    milliseconds: [u64; N],
) -> Result<[Material; N], TestError> {
    let (animation, motion) = moving(from, to)?;
    Ok(milliseconds.map(|ms| at(&animation, &motion, ms)))
}

fn solid(brush: &Brush) -> Result<Color, TestError> {
    match brush {
        Brush::Solid(color) => Ok(*color),
        Brush::Gradient(_) => Err(TestError::Absent("a solid brush")),
    }
}

fn stops(brush: &Brush) -> Result<&[ColorStop], TestError> {
    match brush {
        Brush::Gradient(gradient) => Ok(&gradient.stops),
        Brush::Solid(_) => Err(TestError::Absent("a gradient brush")),
    }
}

fn linear(colors: &[Color]) -> Result<Gradient, NumberError> {
    Gradient::linear(
        Point::new(0.dp(), 0.dp()),
        Point::new(100.dp(), 0.dp()),
        colors,
    )
}

fn glass(color: Color, scale: f64) -> Result<Material, NumberError> {
    let lens = Glass {
        refraction: NonNegativeDp::try_new(10.dp() * scale)?,
        dispersion: NonNegativeDp::try_new(0.7.dp())?,
        rim: NonNegative::try_new(0.14)?,
    };
    let offset = Point::new(0.dp(), 2.dp() * scale);
    Ok(Material::new(color)
        .backdrop(Backdrop::glass(4.dp() * scale, lens)?)
        .border(Stroke::new(color, 2.dp() * scale)?)
        .shadow(Shadow::new(color, 4.dp() * scale, offset)?))
}

#[test]
fn a_material_moves_every_part_together_with_its_colors_in_oklch() -> TestResult {
    let [middle, end] = motion(glass(BLUE, 1.0)?, glass(VIOLET, 2.0)?, [500, 1000])?;
    assert_halfway_from_blue_to_violet(solid(&middle.fill)?, HuePath::Shorter);
    let border = middle.border.required("border")?;
    assert_halfway_from_blue_to_violet(border.color, HuePath::Shorter);
    assert_f64_eq(border.width.get().get(), 3.0);
    let shadow = middle.shadows[0];
    assert_halfway_from_blue_to_violet(shadow.color, HuePath::Shorter);
    assert_f64_eq(shadow.blur.get().get(), 6.0);
    assert_f64_eq(shadow.offset.get().y.get(), 3.0);
    let backdrop = middle.backdrop.required("backdrop")?;
    assert_f64_eq(backdrop.blur.get().get(), 6.0);
    assert_f64_eq(
        backdrop.glass.required("glass")?.refraction.get().get(),
        15.0,
    );
    assert_f64_eq(end.border.required("border")?.width.get().get(), 4.0);
    assert_eq!(end.shadows.len(), 1);
    Ok(())
}

#[test]
fn a_part_that_one_side_lacks_fades_in_and_out_and_is_gone_at_rest() -> TestResult {
    let plain = Material::new(BLUE);
    let bordered = Material::new(BLUE).border(Stroke::new(VIOLET, 4.dp())?);
    let [middle] = motion(plain.clone(), bordered.clone(), [500])?;
    let border = middle.border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.5);
    assert_f64_eq(border.width.get().get(), 2.0);
    let [middle, end] = motion(bordered, plain, [500, 1000])?;
    let border = middle.border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.5);
    assert_f64_eq(border.width.get().get(), 2.0);
    assert_eq!(end.border, None);
    Ok(())
}

#[test]
fn seeking_a_finished_motion_shows_the_parts_that_it_moved_through() -> TestResult {
    let bordered = Material::new(BLUE).border(Stroke::new(VIOLET, 4.dp())?);
    let (animation, motion) = moving(bordered, Material::new(BLUE))?;
    assert_eq!(at(&animation, &motion, 1000).border, None);
    animation.seek(Duration::from_millis(500));
    let border = animation.get().border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.5);
    assert_f64_eq(border.width.get().get(), 2.0);

    let gradient = Material::new(linear(&[VIOLET, GREEN])?);
    let (animation, motion) = moving(gradient, Material::new(BLUE))?;
    let end = at(&animation, &motion, 1000);
    assert!(matches!(end.fill, Brush::Solid(_)));
    animation.seek(Duration::from_millis(500));
    let fill = animation.get().fill;
    assert_halfway_from_blue_to_violet(stops(&fill)?[0].color, HuePath::Shorter);
    Ok(())
}

#[test]
fn a_border_fading_out_turns_back_from_where_it_is() -> TestResult {
    let bordered = Material::new(BLUE).border(Stroke::new(VIOLET, 4.dp())?);
    let (animation, motion) = moving(bordered.clone(), Material::new(BLUE))?;
    motion.advance(Duration::from_millis(500));
    animation.animate_to(bordered, linear_second())?;
    let border = at(&animation, &motion, 500).border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.5);
    assert_f64_eq(border.width.get().get(), 2.0);
    let border = at(&animation, &motion, 1000).border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.75);
    assert_f64_eq(border.width.get().get(), 3.0);
    Ok(())
}

#[test]
fn a_gradient_turned_back_to_a_solid_fill_moves_its_stops_without_a_jump() -> TestResult {
    let gradient = Material::new(linear(&[VIOLET, GREEN])?);
    let (animation, motion) = moving(Material::new(BLUE), gradient)?;
    let before = at(&animation, &motion, 500).fill;
    animation.animate_to(Material::new(BLUE), linear_second())?;
    let turned = at(&animation, &motion, 500).fill;
    for (before, turned) in stops(&before)?.iter().zip(stops(&turned)?) {
        assert_f64_eq(turned.position, before.position);
        assert_eq!(turned.color.to_argb8(), before.color.to_argb8());
    }
    let middle = at(&animation, &motion, 1000).fill;
    let ([l0, c0, _], [l1, c1, _]) = (stops(&before)?[1].color.to_oklch(), BLUE.to_oklch());
    let [l, c, _] = stops(&middle)?[1].color.to_oklch();
    assert_f64_eq(l, f64::midpoint(l0, l1));
    assert_f64_eq(c, f64::midpoint(c0, c1));
    let end = at(&animation, &motion, 1500).fill;
    assert_eq!(solid(&end)?.to_argb8(), BLUE.to_argb8());
    Ok(())
}

#[test]
fn shadows_retargeted_from_one_to_none_to_two_continue_and_grow_from_zero() -> TestResult {
    let first = Shadow::new(BLUE, 8.dp(), Point::new(0.dp(), 4.dp()))?;
    let (animation, motion) = moving(Material::new(BLUE).shadow(first), Material::new(BLUE))?;
    motion.advance(Duration::from_millis(500));
    let second = Shadow::new(BLUE, 4.dp(), Point::new(0.dp(), 2.dp()))?;
    let third = Shadow::new(VIOLET, 2.dp(), Point::new(0.dp(), 1.dp()))?;
    let two = Material::new(BLUE).shadow(second).shadow(third);
    animation.animate_to(two, linear_second())?;
    let turned = at(&animation, &motion, 500).shadows;
    let [continued, grown] = turned[..] else {
        return Err(TestError::Absent("two shadows"));
    };
    assert_f64_eq(continued.color.alpha(), 0.5);
    assert_f64_eq(continued.blur.get().get(), 8.0);
    assert_f64_eq(grown.color.alpha(), 0.0);
    let middle = at(&animation, &motion, 1000).shadows;
    let [continued, grown] = middle[..] else {
        return Err(TestError::Absent("two shadows"));
    };
    assert_f64_eq(continued.color.alpha(), 0.75);
    assert_f64_eq(continued.blur.get().get(), 6.0);
    assert_f64_eq(grown.color.alpha(), 0.5);
    assert_f64_eq(grown.blur.get().get(), 2.0);
    Ok(())
}

#[test]
fn a_backdrop_and_its_glass_grow_from_zero_strength() -> TestResult {
    let [middle] = motion(Material::new(BLUE), glass(BLUE, 1.0)?, [500])?;
    let backdrop = middle.backdrop.required("backdrop")?;
    assert_f64_eq(backdrop.blur.get().get(), 2.0);
    assert_f64_eq(
        backdrop.glass.required("glass")?.refraction.get().get(),
        5.0,
    );
    Ok(())
}

#[test]
fn an_outer_shadow_fades_out_while_an_inset_shadow_fades_in() -> TestResult {
    let outer = Shadow::new(BLUE, 8.dp(), Point::new(0.dp(), 4.dp()))?;
    let inset = Shadow::new(VIOLET, 2.dp(), Point::new(0.dp(), 1.dp()))?.inset(true);
    let from = Material::new(BLUE).shadow(outer);
    let to = Material::new(BLUE).shadow(inset);
    let [middle, end] = motion(from, to, [500, 1000])?;
    let mut shadows: Vec<_> = (middle.shadows.iter())
        .map(|s| (s.inset, s.blur.get().get(), s.color.alpha()))
        .collect();
    shadows.sort_by_key(|&(inset, ..)| inset);
    let [
        (false, outer_blur, outer_alpha),
        (true, inset_blur, inset_alpha),
    ] = shadows[..]
    else {
        return Err(TestError::Absent("one outer and one inset shadow"));
    };
    assert_eq!((outer_blur, inset_blur), (8.0, 2.0));
    assert_f64_eq(outer_alpha, 0.5);
    assert_f64_eq(inset_alpha, 0.5);
    assert_eq!(end.shadows.len(), 1);
    assert!(end.shadows[0].inset);
    Ok(())
}

#[test]
fn a_solid_fill_and_a_gradient_move_through_a_gradient_with_equal_stops() -> TestResult {
    let gradient = linear(&[VIOLET, GREEN])?;
    let [middle] = motion(Material::new(BLUE), Material::new(gradient.clone()), [500])?;
    let painted = stops(&middle.fill)?;
    assert_eq!(painted.len(), 2);
    assert_eq!((painted[0].position, painted[1].position), (0.0, 1.0));
    assert_halfway_from_blue_to_violet(painted[0].color, HuePath::Shorter);
    let [middle, end] = motion(Material::new(gradient), Material::new(BLUE), [500, 1000])?;
    assert_halfway_from_blue_to_violet(stops(&middle.fill)?[0].color, HuePath::Shorter);
    assert!(matches!(end.fill, Brush::Solid(_)));
    Ok(())
}

#[test]
fn a_gradient_with_fewer_stops_repeats_them_and_has_only_its_stops_at_rest() -> TestResult {
    let two = Material::new(linear(&[BLUE, VIOLET])?);
    let four = Material::new(linear(&[BLUE, GREEN, GREEN, VIOLET])?);
    for (from, to, count) in [(two.clone(), four.clone(), 4), (four, two, 2)] {
        let [middle, end] = motion(from, to, [500, 1000])?;
        assert_eq!(stops(&middle.fill)?.len(), 4);
        assert_eq!(stops(&end.fill)?.len(), count);
    }
    Ok(())
}

#[test]
fn a_gradient_of_another_kind_takes_its_kind_and_geometry_at_the_start() -> TestResult {
    let center = Point::new(50.dp(), 50.dp());
    let radial = Gradient::new(
        GradientKind::Radial {
            center,
            radius: 40.dp(),
        },
        [ColorStop::new(0.0, VIOLET), ColorStop::new(1.0, GREEN)],
    );
    let from = Material::new(linear(&[BLUE, GREEN])?);
    let [start] = motion(from, Material::new(radial.clone()), [1])?;
    let Brush::Gradient(gradient) = start.fill else {
        return Err(TestError::Absent("a gradient fill"));
    };
    assert_eq!(gradient.kind, radial.kind);
    Ok(())
}

#[test]
fn keyframes_of_materials_fade_a_part_that_one_frame_lacks() -> TestResult {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, Material::new(BLUE)).easing(Easing::Linear),
        Keyframe::new(
            Duration::from_secs(1),
            Material::new(BLUE).border(Stroke::new(VIOLET, 4.dp())?),
        ),
    ])?;
    let motion = Motion::default();
    let animation = motion.animation(Material::new(BLUE))?;
    animation.play(frames, Playback::default());
    motion.advance(Duration::from_millis(500));
    let border = animation.get().border.required("border")?;
    assert_f64_eq(border.color.alpha(), 0.5);
    assert_f64_eq(border.width.get().get(), 2.0);
    Ok(())
}

#[test]
fn keyframes_paint_each_segment_with_the_gradient_kind_of_that_segment() -> TestResult {
    let linear = |to: f64| -> Result<Material, NumberError> {
        let from = Point::new(0.dp(), 0.dp());
        let gradient = Gradient::linear(from, Point::new(Dp::new(to), 0.dp()), &[BLUE, VIOLET])?;
        Ok(Material::new(gradient))
    };
    let radial = Material::new(Gradient {
        kind: GradientKind::Radial {
            center: Point::new(50.dp(), 50.dp()),
            radius: 30.dp(),
        },
        ..Gradient::linear(Point::default(), Point::default(), &[BLUE, VIOLET])?
    });
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, linear(100.0)?).easing(Easing::Linear),
        Keyframe::new(Duration::from_secs(1), linear(200.0)?).easing(Easing::Linear),
        Keyframe::new(Duration::from_secs(2), radial),
    ])?;
    let motion = Motion::default();
    let animation = motion.animation(linear(100.0)?)?;
    animation.play(frames, Playback::default());
    motion.advance(Duration::from_millis(500));
    let Brush::Gradient(gradient) = animation.get().fill else {
        return Err(TestError::Absent("a gradient fill"));
    };
    let GradientKind::Linear { to, .. } = gradient.kind else {
        return Err(TestError::Absent(
            "the linear gradient of the first segment",
        ));
    };
    assert_f64_eq(to.x.get(), 150.0);
    Ok(())
}
