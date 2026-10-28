// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{AnimationError, MotionSpec, Tween};
use crate::ui::ModifierValue;
use crate::view::CompositionError;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

/// The causes of the failures in the next frame of `view`.
fn next_failures(view: &mut View) -> Result<Vec<ComponentError>, TestError> {
    failure_causes(advance_with(
        &mut Scene::default(),
        view,
        WINDOW,
        &mut Metrics,
    ))
}

#[test]
fn two_siblings_with_one_key_fail_their_composition() -> TestResult {
    let doubled = State::new(false);
    let copies = doubled.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = if copies.get() { 2 } else { 1 };
        for _ in 0..count {
            ui.key("row", Modifier::new().tag("row"), |ui| {
                ui.spacer(10.dp(), 10.dp(), Modifier::new());
            });
        }
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    doubled.set(true);
    assert!(matches!(
        next_failures(&mut view)?.as_slice(),
        [ComponentError::Layout(LayoutError::DuplicateKey(_))]
    ));
    assert_eq!(view.bounds_of("row"), None);
    Ok(())
}

#[test]
fn a_border_width_that_is_not_finite_fails_its_composition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let border = Modifier::new().border(Color::WHITE, Dp::new(f64::NAN), Corners::default());
        ui.spacer(10.dp(), 10.dp(), border);
    });
    assert!(matches!(
        next_failures(&mut view)?.as_slice(),
        [ComponentError::Layout(LayoutError::Modifier {
            value: ModifierValue::BorderWidth,
            cause: NumberError::NotFinite,
            ..
        })]
    ));
    Ok(())
}

#[test]
fn an_animation_target_that_is_not_finite_fails_the_composition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        match ui.animate("width", Dp::new(f64::NAN), MotionSpec::Instant) {
            Ok(width) => ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(&width)),
            Err(error) => ui.fail(error),
        }
    });
    assert_eq!(
        next_failures(&mut view)?,
        [ComponentError::Composition(CompositionError::Animation(
            AnimationError::Value
        ))]
    );
    Ok(())
}

#[test]
fn a_clip_that_its_transition_cannot_take_fails_its_composition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let clipped = Modifier::new()
            .clip(Dp::new(f64::NAN))
            .transition(Tween::new(std::time::Duration::from_secs(1)));
        ui.spacer(10.dp(), 10.dp(), clipped);
    });
    assert_eq!(
        next_failures(&mut view)?,
        [ComponentError::Paint(AnimationError::Value)]
    );
    Ok(())
}

#[test]
fn a_negative_weight_fails_its_composition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.spacer(20.dp(), 10.dp(), Modifier::new().weight(-1.0));
    });
    assert!(matches!(
        next_failures(&mut view)?.as_slice(),
        [ComponentError::Layout(LayoutError::Weight {
            cause: NumberError::Negative,
            ..
        })]
    ));
    Ok(())
}

#[test]
fn an_infinite_weight_fails_its_composition() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let weight = Modifier::new().weight_up_to(f64::INFINITY);
        ui.spacer(20.dp(), 10.dp(), weight);
    });
    assert!(matches!(
        next_failures(&mut view)?.as_slice(),
        [ComponentError::Layout(LayoutError::Weight {
            cause: NumberError::NotFinite,
            ..
        })]
    ));
    Ok(())
}
