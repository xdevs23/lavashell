// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::style::StateStyles;
use crate::style::{BorderStyle, INTERACTION_STYLE, InteractionAppearance, InteractionStyle};
use crate::units::NonNegativeDp;

fn tap(view: &mut View, tag: &str) -> TestResult {
    let at = center(view.tagged_bounds(tag)?);
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
    Ok(())
}

fn buttons() -> View {
    View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            for tag in ["first", "second"] {
                ui.spacer(100.dp(), 40.dp(), Modifier::new().tag(tag).on_click(|| {}));
            }
        });
    })
}

#[test]
fn a_tap_focuses_a_node_without_making_its_focus_visible() -> TestResult {
    let mut scene = Scene::default();
    let mut view = buttons();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "second")?;
    let state = view.tagged_state("second")?;
    assert!(state.focus.focused);
    assert!(!state.focus.visible);
    Ok(())
}

#[test]
fn tab_arrows_and_other_keys_make_focus_visible() -> TestResult {
    let keys = [
        TAB,
        InputEvent::Key {
            key: Key::Down,
            shift: false,
        },
        InputEvent::Key {
            key: Key::Character('x'),
            shift: false,
        },
        InputEvent::Shortcut(Shortcut::control(Key::Character('k'))),
        InputEvent::Text(String::from("x")),
    ];
    for key in keys {
        let mut scene = Scene::default();
        let mut view = buttons();
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        tap(&mut view, "first")?;
        view.input(key);
        let focused = [view.tagged_state("first")?, view.tagged_state("second")?];
        assert!(focused.iter().any(|state| state.focus.focused));
        for state in focused {
            assert_eq!(state.focus.visible, state.focus.focused);
        }
    }
    Ok(())
}

#[test]
fn a_tap_after_keys_hides_focus_again() -> TestResult {
    let mut scene = Scene::default();
    let mut view = buttons();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(TAB);
    assert!(view.tagged_state("first")?.focus.visible);
    tap(&mut view, "first")?;
    let state = view.tagged_state("first")?;
    assert!(state.focus.focused);
    assert!(!state.focus.visible);
    Ok(())
}

#[test]
fn a_context_press_after_keys_hides_focus_again() -> TestResult {
    let mut scene = Scene::default();
    let mut view = buttons();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(TAB);
    view.input(InputEvent::Context(center(view.tagged_bounds("second")?)));
    let state = view.tagged_state("second")?;
    assert!(state.focus.focused);
    assert!(!state.focus.visible);
    Ok(())
}

#[test]
fn focus_that_falls_back_to_an_ancestor_keeps_the_answer_of_the_last_input() -> TestResult {
    for keys in [false, true] {
        let shown = State::new(true);
        let child = shown.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            ui.layout(
                Axis::Vertical,
                Modifier::new().tag("parent").on_click(|| {}),
                |ui| {
                    if child.get() {
                        ui.spacer(
                            100.dp(),
                            40.dp(),
                            Modifier::new().tag("child").on_click(|| {}),
                        );
                    }
                },
            );
        });
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        if keys {
            view.input(TAB);
            view.input(TAB);
        } else {
            tap(&mut view, "child")?;
        }
        assert!(view.tagged_state("child")?.focus.focused);
        shown.set(false);
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        let state = view.tagged_state("parent")?;
        assert!(state.focus.focused);
        assert_eq!(state.focus.visible, keys);
    }
    Ok(())
}

#[test]
fn a_modal_that_opens_keeps_the_answer_of_the_last_input() -> TestResult {
    for keys in [false, true] {
        let open = State::new(false);
        let modal = open.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new().tag("background").on_click(|| {}),
                );
                if modal.get() {
                    ui.layout(Axis::Vertical, Modifier::new().modal(), |ui| {
                        ui.spacer(
                            100.dp(),
                            40.dp(),
                            Modifier::new().tag("dialog").on_click(|| {}),
                        );
                    });
                }
            });
        });
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        if keys {
            view.input(TAB);
        } else {
            tap(&mut view, "background")?;
        }
        open.set(true);
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        let state = view.tagged_state("dialog")?;
        assert!(state.focus.focused);
        assert_eq!(state.focus.visible, keys);
    }
    Ok(())
}

#[test]
fn a_modal_open_before_any_input_shows_its_focus() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new().modal(), |ui| {
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new().tag("dialog").on_click(|| {}),
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let state = view.tagged_state("dialog")?;
    assert!(state.focus.focused);
    assert!(state.focus.visible);
    Ok(())
}

#[test]
fn a_text_field_and_a_text_area_show_visible_focus_after_a_tap() -> TestResult {
    for multiline in [false, true] {
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let modifier = Modifier::new().tag("field").size(200.dp(), 40.dp());
            if multiline {
                ui.text_area(String::new(), TextFieldOptions::default(), modifier, |_| {});
            } else {
                ui.text_input(String::new(), TextFieldOptions::default(), modifier, |_| {});
            }
        });
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        tap(&mut view, "field")?;
        let state = view.tagged_state("field")?;
        assert!(state.focus.focused);
        assert!(state.focus.visible);
    }
    Ok(())
}

#[test]
fn the_focus_ring_of_the_theme_paints_only_where_focus_is_visible() -> TestResult {
    let style = InteractionStyle {
        focus: Some(BorderStyle {
            color: Color::WHITE,
            width: crate::constant!(NonNegativeDp::try_new(Dp::new(5.0))),
            radius: 4.dp(),
        }),
        ..InteractionStyle::default()
    };
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.provide(&INTERACTION_STYLE, style.clone(), |ui| {
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new().tag("button").on_click(|| {}),
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "button")?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(stroke_widths(&scene.commands()).is_empty());
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(stroke_widths(&scene.commands()), [5.dp()]);
    Ok(())
}

#[test]
fn a_focus_within_border_paints_only_where_focus_inside_is_visible() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Axis::Vertical,
            Modifier::new().focus_within(BorderStyle {
                color: Color::WHITE,
                width: crate::constant!(NonNegativeDp::try_new(Dp::new(3.0))),
                radius: 4.dp(),
            }),
            |ui| {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new().tag("button").on_click(|| {}),
                );
            },
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "button")?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(stroke_widths(&scene.commands()).is_empty());
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(stroke_widths(&scene.commands()), [3.dp()]);
    Ok(())
}

#[test]
fn a_state_style_reading_focus_visible_changes_without_composing_again() -> TestResult {
    let styles = StateStyles::new(|state| {
        InteractionAppearance::default().outline(if state.focus.visible {
            BorderStyle {
                color: Color::WHITE,
                width: crate::constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                radius: 4.dp(),
            }
        } else {
            BorderStyle::NONE
        })
    });
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let styles = styles.clone();
        ui.scoped("control", Modifier::new(), move |ui| {
            ui.spacer(
                100.dp(),
                40.dp(),
                Modifier::new()
                    .tag("button")
                    .state_styles(styles.clone())
                    .on_click(|| {}),
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "button")?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(stroke_widths(&scene.commands()).is_empty());
    let compositions = view.stats().root_compositions;
    view.input(InputEvent::Key {
        key: Key::Down,
        shift: false,
    });
    assert!(view.needs_visual_frame());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.stats().root_compositions, compositions);
    assert_eq!(stroke_widths(&scene.commands()), [2.dp()]);
    Ok(())
}

fn stroke_rects(commands: &[DrawCommand]) -> Vec<Rect> {
    let (mut transform, mut saved, mut rects) = (Affine::IDENTITY, Vec::new(), Vec::new());
    for command in commands {
        match command {
            DrawCommand::Save => saved.push(transform),
            DrawCommand::Restore => transform = saved.pop().unwrap_or(Affine::IDENTITY),
            DrawCommand::Transform(inner) => transform = transform.then(*inner),
            DrawCommand::Stroke { shape, .. } => {
                let bounds = shape.bounds();
                let origin = transform.map_point(Point::new(bounds.x, bounds.y));
                rects.push(Rect::new(origin.x, origin.y, bounds.width, bounds.height));
            }
            _ => {}
        }
    }
    rects
}

#[test]
fn the_focus_ring_follows_an_animated_offset_once() -> TestResult {
    use crate::animation::Animation;
    use std::time::Duration;
    let rings = |animated: bool| -> Result<Vec<Rect>, TestError> {
        let style = InteractionStyle {
            focus: Some(BorderStyle {
                color: Color::WHITE,
                width: crate::constant!(NonNegativeDp::try_new(Dp::new(2.0))),
                radius: 4.dp(),
            }),
            ..InteractionStyle::default()
        };
        let offset = Animation::new(Point::default())?;
        let moved = offset.clone();
        let mut scene = Scene::default();
        let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
            let modifier = if animated {
                Modifier::new().animated_offset(moved.clone())
            } else {
                Modifier::new().offset(30.dp(), 0.dp())
            };
            ui.provide(&INTERACTION_STYLE, style.clone(), |ui| {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new().tag("button").on_click(|| {}).then(modifier),
                );
            });
        });
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        offset.animate_to(
            Point::new(30.dp(), 0.dp()),
            crate::spring!(Duration::from_millis(450), 0.85),
        )?;
        tap(&mut view, "button")?;
        view.input(TAB);
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 5000)?;
        Ok(stroke_rects(&scene.at(5000).commands()))
    };
    assert_eq!(rings(true)?, rings(false)?);
    assert_eq!(rings(false)?.len(), 1);
    Ok(())
}
