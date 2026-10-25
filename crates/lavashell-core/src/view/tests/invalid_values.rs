// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{AnimationError, MotionSpec, Tween};
use crate::ui::ModifierValue;
use crate::view::CompositionError;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

#[test]
fn two_siblings_with_one_key_fail_the_layout_and_keep_the_view() -> TestResult {
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
    let result = advance_with(&mut scene, &mut view, WINDOW, &mut Metrics);
    assert!(matches!(
        result,
        Err(TestError::Frame(FrameError::Layout(
            LayoutError::DuplicateKey(_)
        )))
    ));
    assert_eq!(
        view.tagged_bounds("row")?.size(),
        Size::new(10.dp(), 10.dp())
    );
    Ok(())
}

#[test]
fn a_border_width_that_is_not_finite_fails_the_layout() {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let border = Modifier::new().border(Color::WHITE, Dp::new(f64::NAN), Corners::default());
        ui.spacer(10.dp(), 10.dp(), border);
    });
    let result = advance_with(&mut scene, &mut view, WINDOW, &mut Metrics);
    assert!(matches!(
        result,
        Err(TestError::Frame(FrameError::Layout(
            LayoutError::Modifier {
                value: ModifierValue::BorderWidth,
                cause: NumberError::NotFinite,
                ..
            }
        )))
    ));
}

#[test]
fn an_animation_target_that_is_not_finite_fails_the_composition() {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        match ui.animate("width", Dp::new(f64::NAN), MotionSpec::Instant) {
            Ok(width) => ui.spacer(Dp::ZERO, 10.dp(), Modifier::new().width(&width)),
            Err(error) => ui.fail(error),
        }
    });
    let result = advance_with(&mut scene, &mut view, WINDOW, &mut Metrics);
    assert!(matches!(
        result,
        Err(TestError::Frame(FrameError::Layout(
            LayoutError::Composition(CompositionError::Animation(AnimationError::Value))
        )))
    ));
}

#[test]
fn a_clip_that_its_transition_cannot_take_fails_the_paint() {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let clipped = Modifier::new()
            .clip(Dp::new(f64::NAN))
            .transition(Tween::new(std::time::Duration::from_secs(1)));
        ui.spacer(10.dp(), 10.dp(), clipped);
    });
    let result = advance_with(&mut scene, &mut view, WINDOW, &mut Metrics);
    assert!(matches!(
        result,
        Err(TestError::Frame(FrameError::Paint(AnimationError::Value)))
    ));
}
