// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::Tween;
use crate::float_asserts::assert_f64_eq;
use crate::preferences::REDUCED_MOTION;
use crate::style::{InteractionAppearance, StateStyles};
use crate::units::NonNegativeDp;
use std::time::Duration;

pub(super) const SMALL_WINDOW_SIZE: Size = Size::new(Dp::new(200.0), Dp::new(100.0));

const FOCUSED: NonNegativeDp = crate::constant!(NonNegativeDp::try_new(Dp::new(2.0)));

fn setup(scene: &mut Scene) -> Result<(View, State<bool>, State<u32>), TestError> {
    let enabled = State::new(true);
    let clicks = State::new(0);
    let flag = enabled.clone();
    let count = clicks.clone();
    let styles = StateStyles::new(|state| {
        let alpha = if state.disabled {
            0.0
        } else if state.pointer.active {
            1.0
        } else if state.pointer.hovered {
            0.5
        } else {
            0.0
        };
        InteractionAppearance::default()
            .tint(Color::WHITE.with_alpha(alpha), 6.dp())
            .outline(crate::style::BorderStyle {
                color: Color::WHITE,
                width: if state.focus.focused {
                    FOCUSED
                } else {
                    NonNegativeDp::ZERO
                },
                radius: 6.dp(),
            })
    })
    .motion(Tween::new(Duration::from_millis(100)).preserve_velocity(false));
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let count = count.clone();
        let enabled = flag.get();
        let styles = styles.clone();
        ui.key("control", Modifier::new(), move |ui| {
            let count = count.clone();
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new()
                    .tag("button")
                    .enabled(enabled)
                    .state_styles(styles.clone())
                    .on_click(move || count.update(|count| *count += 1)),
            );
        });
    });
    advance(scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    Ok((view, enabled, clicks))
}

pub(super) fn tint(view: &View, tag: &str) -> Result<Color, TestError> {
    let id = view
        .nodes
        .iter()
        .find(|node| node.tag.as_deref() == Some(tag))
        .required("a node with that tag")?
        .id;
    let tint = view.interaction_visuals[&id].animation.get().tint;
    Ok(tint.required("a tint")?.color.color)
}

#[test]
fn focused_button_space_activates_without_scrolling_its_parent() -> TestResult {
    let clicks = State::new(0);
    let count = clicks.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(
            Axis::Vertical,
            Modifier::new()
                .focusable(false)
                .height(100.dp())
                .tag("scroll"),
            |ui| {
                let count = count.clone();
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag("button")
                        .on_click(move || count.update(|value| *value += 1)),
                );
                ui.spacer(100.dp(), 500.dp(), Modifier::new());
            },
        );
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    let before = view.tagged_bounds("button")?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    view.input(InputEvent::KeyRelease(Key::Space));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_eq!(clicks.get(), 1);
    assert_eq!(view.tagged_bounds("button")?, before);
    Ok(())
}

#[test]
fn pointer_active_follows_capture_inside_and_window_focus_cancels() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, _, clicks) = setup(&mut scene)?;
    let point = Point::new(20.dp(), 20.dp());
    view.input(InputEvent::Press(point));
    assert!(view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::Move(Point::new(150.dp(), 80.dp())));
    assert!(!view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::Move(point));
    assert!(view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::WindowFocus(false));
    assert!(!view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::Release(point));
    assert_eq!(clicks.get(), 0);
    view.input(InputEvent::Move(point));
    assert!(view.tagged_state("button")?.pointer.hovered);
    assert!(!view.tagged_state("button")?.focus.focused);
    view.input(InputEvent::WindowFocus(true));
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn keyboard_release_repeat_focus_loss_and_disabled_controls_retire_active_state() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, enabled, clicks) = setup(&mut scene)?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    for _ in 0..3 {
        view.input(InputEvent::Key {
            key: Key::Space,
            shift: false,
        });
    }
    assert_eq!(clicks.get(), 1);
    assert!(view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::KeyRelease(Key::Enter));
    assert!(view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::KeyRelease(Key::Space));
    assert!(!view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    view.input(InputEvent::WindowFocus(false));
    assert!(!view.tagged_state("button")?.focus.focused);
    assert!(view.keyboard_press.is_none());
    view.input(InputEvent::WindowFocus(true));
    enabled.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    assert!(view.tagged_state("button")?.disabled);
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(clicks.get(), 2);
    Ok(())
}

#[test]
fn retained_control_animates_interruptions_and_reduced_motion_without_recomposition() -> TestResult
{
    let mut scene = Scene::default();
    let (mut view, _, _) = setup(&mut scene)?;
    let compositions = view.stats().root_compositions;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 50)?;
    let halfway = tint(&view, "button")?.alpha();
    assert!((halfway - 0.25).abs() < 0.01);
    view.input(InputEvent::Press(Point::new(20.dp(), 20.dp())));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 50)?;
    assert!((tint(&view, "button")?.alpha() - halfway).abs() < 0.01);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    assert!(tint(&view, "button")?.alpha() > halfway);
    assert_eq!(view.stats().root_compositions, compositions);
    view.provide(&REDUCED_MOTION, true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), 1.0);
    view.input(InputEvent::Release(Point::new(20.dp(), 20.dp())));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 100)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.5);
    assert!(!view.motion.is_active());
    Ok(())
}

#[test]
fn pointer_cancellation_preserves_keyboard_focus_without_activating_the_old_capture() -> TestResult
{
    let mut scene = Scene::default();
    let (mut view, _, clicks) = setup(&mut scene)?;
    let point = Point::new(20.dp(), 20.dp());
    view.input(InputEvent::Press(point));
    view.input(InputEvent::WindowFocus(true));
    assert!(view.tagged_state("button")?.pointer.active);
    view.input(InputEvent::CancelPointer);
    let state = view.tagged_state("button")?;
    assert!(state.focus.focused);
    assert!(!state.pointer.hovered && !state.pointer.active);
    view.input(InputEvent::Release(point));
    assert_eq!(clicks.get(), 0);
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn state_style_color_interpolation_chooses_the_hue_path_of_the_tint() -> TestResult {
    use crate::animation::Easing;
    use crate::color::samples::{BLUE, VIOLET, assert_halfway_from_blue_to_violet};
    use crate::color::{ColorInterpolation, HuePath};
    let styles = StateStyles::new(|state| {
        InteractionAppearance::default()
            .tint(if state.pointer.hovered { VIOLET } else { BLUE }, 0.dp())
    })
    .motion(Tween::new(Duration::from_millis(100)).easing(Easing::Linear))
    .color_interpolation(ColorInterpolation::Oklch(HuePath::Longer));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .tag("button")
                .state_styles(styles.clone())
                .on_click(|| {}),
        );
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 50)?;
    assert_halfway_from_blue_to_violet(tint(&view, "button")?, HuePath::Longer);
    Ok(())
}

#[test]
fn a_resting_tint_that_only_changes_its_interpolation_keeps_its_color() -> TestResult {
    use crate::color::samples::{VIOLET, hue_distance};
    use crate::color::{ColorInterpolation, HuePath};
    let longer = State::new(false);
    let path = longer.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let interpolation = ColorInterpolation::Oklch(if path.get() {
            HuePath::Longer
        } else {
            HuePath::Shorter
        });
        let styles = StateStyles::new(|_| InteractionAppearance::default().tint(VIOLET, 0.dp()))
            .motion(Tween::new(Duration::from_millis(100)))
            .color_interpolation(interpolation);
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .tag("button")
                .state_styles(styles)
                .on_click(|| {}),
        );
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    longer.set(true);
    for time in [0, 50, 100] {
        advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, time)?;
        let hue = tint(&view, "button")?.to_oklch()[2];
        assert!(
            hue_distance(hue, VIOLET.to_oklch()[2]) < 1e-3,
            "{time}: {hue}"
        );
    }
    Ok(())
}

#[test]
fn modal_changes_repaint_retained_appearance_in_the_first_frame_with_unchanged_focus() -> TestResult
{
    let visible = State::new(false);
    let modal = visible.clone();
    let styles = StateStyles::new(|state| {
        let alpha = if state.disabled {
            0.8
        } else if state.pointer.active {
            0.6
        } else if state.pointer.hovered {
            0.4
        } else {
            0.2
        };
        InteractionAppearance::default().tint(Color::WHITE.with_alpha(alpha), 0.dp())
    });
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
            let styles = styles.clone();
            ui.key("background", Modifier::new(), move |ui| {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag("button")
                        .state_styles(styles.clone())
                        .on_click(|| {})
                        .focusable(false),
                );
            });
            let modal = modal.clone();
            ui.key("modal", Modifier::new(), move |ui| {
                if modal.get() {
                    ui.spacer(10.dp(), 10.dp(), Modifier::new().modal().focusable(false));
                }
            });
        });
    });
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    view.input(InputEvent::Press(Point::new(20.dp(), 20.dp())));
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.6);
    assert_eq!(view.focus(), None);
    visible.set(true);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_eq!(view.focus(), None);
    assert!(view.tagged_state("button")?.disabled);
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.8);
    visible.set(false);
    advance(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert_eq!(view.focus(), None);
    assert!(!view.tagged_state("button")?.disabled);
    assert_f64_eq(tint(&view, "button")?.alpha(), 0.2);
    Ok(())
}

#[test]
fn focusable_scroll_container_draws_its_requested_focus_appearance() -> TestResult {
    let styles = StateStyles::new(|state| {
        InteractionAppearance::default().outline(crate::style::BorderStyle {
            color: Color::WHITE,
            width: if state.focus.focused {
                FOCUSED
            } else {
                NonNegativeDp::ZERO
            },
            radius: 4.dp(),
        })
    });
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(
            Axis::Vertical,
            Modifier::new()
                .state_styles(styles.clone())
                .tag("scroll")
                .size(100.dp(), 40.dp()),
            |ui| {
                ui.spacer(100.dp(), 300.dp(), Modifier::new());
            },
        );
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    show_frame(&mut scene, &mut view, SMALL_WINDOW_SIZE, 0)?;
    assert!(view.tagged_state("scroll")?.focus.focused);
    assert!(scene.commands().iter().any(
        |command| matches!(command, DrawCommand::Stroke { stroke, .. }
                if stroke.width.get() == 2.dp())
    ));
    Ok(())
}
