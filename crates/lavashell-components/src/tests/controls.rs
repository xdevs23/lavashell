// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::view::{ComponentError, CompositionError};
use lavashell_test_platform::failure_causes;

#[test]
fn button_behavior_survives_caller_styling_and_respects_clip_and_disabled() -> Result<(), TestError>
{
    let clicks = State::new(0);
    let enabled = State::new(true);
    let counter = clicks.clone();
    let active = enabled.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.button(
            Modifier::new()
                .enabled(active.get())
                .clip(Corners::all(20.dp()))
                .background(Color::WHITE)
                .padding(10.dp()),
            counter.action(|n| *n += 1),
            |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    let size = Size::new(100.dp(), 100.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    click(&mut view, 1.dp(), 1.dp());
    assert_eq!(clicks.get(), 0, "rounded clipping excludes the corner");
    click(&mut view, 20.dp(), 5.dp());
    assert_eq!(clicks.get(), 1, "the component includes styled padding");
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    assert_eq!(clicks.get(), 2);
    enabled.set(false);
    render(&mut view, viewport, &mut Metrics)?;
    click(&mut view, 20.dp(), 20.dp());
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(clicks.get(), 2);
    Ok(())
}

#[test]
fn styled_slider_keeps_capture_and_clamps_after_leaving_the_control() -> Result<(), TestError> {
    let value = State::new(0.0);
    let controlled = value.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let read = controlled.clone();
        ui.slider(
            move || read.get(),
            None,
            Modifier::new().background(Color::WHITE),
            controlled.handler(|v, changed| *v = changed),
            |ui, _| {
                ui.spacer(100.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    view.input(InputEvent::Press(Point {
        x: 25.dp(),
        y: 10.dp(),
    }));
    assert_close(value.get(), 0.25);
    view.input(InputEvent::Move(Point {
        x: 150.dp(),
        y: 80.dp(),
    }));
    assert_close(value.get(), 1.0);
    view.input(InputEvent::Release(Point {
        x: -50.dp(),
        y: 80.dp(),
    }));
    assert_close(value.get(), 0.0);
    Ok(())
}

#[test]
fn a_slider_page_that_is_not_finite_fails_its_composition() -> Result<(), TestError> {
    let mut view = View::new(lavashell_test_platform::facts(), |ui| {
        ui.slider(|| 0.5, Some(f64::NAN), Modifier::new(), |_| {}, |_, _| {});
    });
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    assert_eq!(
        failure_causes(render(&mut view, viewport, &mut Metrics))?,
        [ComponentError::Composition(CompositionError::Number(
            NumberError::NotFinite
        ))]
    );
    Ok(())
}

fn slider(value: &State<f64>) -> Result<(View, Viewport), TestError> {
    let controlled = value.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let read = controlled.clone();
        ui.slider(
            move || read.get(),
            None,
            Modifier::new(),
            controlled.handler(|v, changed| *v = changed),
            |ui, _| {
                ui.spacer(100.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    Ok((view, viewport))
}

#[test]
fn a_slider_restores_its_value_from_before_a_drag_that_is_cancelled() -> Result<(), TestError> {
    let value = State::new(0.5);
    let (mut view, viewport) = slider(&value)?;
    let finger = TouchId { seat: 0, point: 1 };
    view.input(InputEvent::TouchDown(TouchSample {
        id: finger,
        at: Point::new(25.dp(), 10.dp()),
        kind: PointerKind::Finger,
    }));
    assert_close(value.get(), 0.25);
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchMove(TouchSample {
        id: finger,
        at: Point::new(80.dp(), 10.dp()),
        kind: PointerKind::Finger,
    }));
    assert_close(value.get(), 0.8);
    view.input(InputEvent::TouchCancel(finger));
    assert_close(value.get(), 0.5);
    Ok(())
}

#[test]
fn a_slider_restores_the_value_from_before_its_drags_when_the_last_running_drag_is_cancelled()
-> Result<(), TestError> {
    let value = State::new(0.5);
    let (mut view, viewport) = slider(&value)?;
    let (first, second) = (TouchId { seat: 0, point: 1 }, TouchId { seat: 0, point: 2 });
    view.input(InputEvent::TouchDown(TouchSample {
        id: first,
        at: Point::new(25.dp(), 10.dp()),
        kind: PointerKind::Finger,
    }));
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: second,
        at: Point::new(60.dp(), 10.dp()),
        kind: PointerKind::Finger,
    }));
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchMove(TouchSample {
        id: first,
        at: Point::new(80.dp(), 10.dp()),
        kind: PointerKind::Finger,
    }));
    assert_close(value.get(), 0.8);
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchCancel(first));
    assert_close(value.get(), 0.8);
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchCancel(second));
    assert_close(value.get(), 0.5);
    Ok(())
}

#[test]
fn controls_inherit_typography_without_inventing_decorations() -> Result<(), TestError> {
    let style = TextStyle::new("application-body", 17.sp(), Color::rgb(0x12_34_56))
        .weight(FontWeight::SEMI_BOLD);
    let expected = style.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.provide(&TEXT_STYLE, style.clone(), |ui| {
            ui.column(Modifier::new(), |ui| {
                ui.button(
                    Modifier::new(),
                    || {},
                    |ui| {
                        ui.text("Label", Modifier::new());
                    },
                );
                ui.text_field(
                    "Value",
                    TextFieldOptions::default(),
                    Modifier::new(),
                    |_| {},
                );
            });
        });
    });
    let mut tree = SceneTree::default();
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    (view.show_frame(&mut tree, viewport, &mut Metrics)).without_failures()?;
    let commands: Vec<_> = tree.at(std::time::Duration::ZERO).leaf_commands().collect();
    let text: Vec<_> = (commands.iter())
        .filter_map(|command| match command {
            DrawCommand::Text { style, .. } => Some(style),
            _ => None,
        })
        .collect();
    assert_eq!(text.len(), 2);
    assert!(text.iter().all(|style| style.family == expected.family
        && style.size == TextScaling::default().to_dp(expected.size)
        && style.weight == expected.weight
        && style.color == expected.color));
    assert!(!commands.iter().any(|command| matches!(
        command,
        DrawCommand::Fill { .. } | DrawCommand::Stroke { .. }
    )));
    Ok(())
}

#[test]
fn component_activation_follows_its_visual_offset() -> Result<(), TestError> {
    let clicks = State::new(0);
    let counter = clicks.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.icon_button(
            &BLANK,
            IconOptions::default(),
            Modifier::new()
                .tag("shifted")
                .size(20.dp(), 20.dp())
                .offset(40.dp(), 30.dp()),
            counter.action(|n| *n += 1),
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.bounds_of("shifted"),
        Some(Rect::new(40.dp(), 30.dp(), 20.dp(), 20.dp()))
    );
    click(&mut view, 5.dp(), 5.dp());
    assert_eq!(clicks.get(), 0);
    click(&mut view, 45.dp(), 35.dp());
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn a_scrollbar_of_a_row_that_scrolls_drags_the_row_sideways() -> Result<(), TestError> {
    let scroll = ScrollState::new(Axis::Horizontal);
    let (row, bar) = (scroll.clone(), scroll.clone());
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.column(Modifier::new(), |ui| {
            ui.scroll(row.clone(), Modifier::new().width(200.dp()), |ui| {
                for _ in 0..4 {
                    ui.spacer(100.dp(), 40.dp(), Modifier::new());
                }
            });
            ui.scrollbar(bar.clone(), Modifier::new(), |ui, bar| {
                ui.spacer(200.dp(), 10.dp(), Modifier::new());
                ui.spacer(bar.thumb_length(200.dp()), 10.dp(), bar.follow(200.dp()));
            });
        });
    });
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(5.dp(), 45.dp())));
    view.input(InputEvent::Move(Point::new(55.dp(), 45.dp())));
    view.input(InputEvent::Release(Point::new(55.dp(), 45.dp())));
    assert_eq!(scroll.offset(), 100.dp());
    Ok(())
}

#[test]
fn each_finger_on_a_scrollbar_drags_from_the_point_where_it_grabbed_the_thumb()
-> Result<(), TestError> {
    let scroll = ScrollState::new(Axis::Horizontal);
    let (row, bar) = (scroll.clone(), scroll.clone());
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.column(Modifier::new(), |ui| {
            ui.scroll(row.clone(), Modifier::new().width(200.dp()), |ui| {
                for _ in 0..4 {
                    ui.spacer(100.dp(), 40.dp(), Modifier::new());
                }
            });
            ui.scrollbar(bar.clone(), Modifier::new(), |ui, bar| {
                ui.spacer(200.dp(), 10.dp(), Modifier::new());
                ui.spacer(bar.thumb_length(200.dp()), 10.dp(), bar.follow(200.dp()));
            });
        });
    });
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    render(&mut view, viewport, &mut Metrics)?;
    let (first, second) = (TouchId { seat: 0, point: 1 }, TouchId { seat: 0, point: 2 });
    view.input(InputEvent::TouchDown(TouchSample {
        id: first,
        at: Point::new(5.dp(), 45.dp()),
        kind: PointerKind::Finger,
    }));
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: second,
        at: Point::new(150.dp(), 45.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: second,
        at: Point::new(150.dp(), 45.dp()),
        kind: PointerKind::Finger,
    }));
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::TouchMove(TouchSample {
        id: first,
        at: Point::new(55.dp(), 45.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 100.dp());
    Ok(())
}

#[test]
fn an_arrow_key_moves_a_slider_by_one_pixel_of_its_track_and_page_keys_do_nothing()
-> Result<(), TestError> {
    let value = State::new(0.5);
    let controlled = value.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let read = controlled.clone();
        ui.slider(
            move || read.get(),
            None,
            Modifier::new(),
            controlled.handler(|v, changed| *v = changed),
            |ui, _| {
                ui.spacer(200.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    let scale = 2.0;
    let density = Density::from_scale(scale).ok_or(TestError::Density(scale))?;
    let viewport = Viewport::new(Size::new(300.dp(), 100.dp()), density);
    for _ in 0..2 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    click(&mut view, 10.dp(), 10.dp());
    assert_close(value.get(), 0.05);
    view.input(InputEvent::Key {
        key: Key::Right,
        shift: false,
    });
    assert_close(value.get(), 0.0525);
    for page in [Key::PageUp, Key::PageDown] {
        view.input(InputEvent::Key {
            key: page,
            shift: false,
        });
    }
    assert_close(value.get(), 0.0525);
    Ok(())
}

#[test]
fn a_page_key_moves_a_slider_by_the_page_that_the_caller_gives() -> Result<(), TestError> {
    let value = State::new(0.5);
    let controlled = value.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let read = controlled.clone();
        ui.slider(
            move || read.get(),
            Some(0.25),
            Modifier::new(),
            controlled.handler(|v, changed| *v = changed),
            |ui, _| {
                ui.spacer(200.dp(), 20.dp(), Modifier::new());
            },
        );
    });
    let viewport = Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE);
    for _ in 0..2 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    click(&mut view, 10.dp(), 10.dp());
    assert_close(value.get(), 0.05);
    for (key, expected) in [(Key::PageUp, 0.3), (Key::PageDown, 0.05)] {
        view.input(InputEvent::Key { key, shift: false });
        assert_close(value.get(), expected);
    }
    Ok(())
}

#[test]
fn a_scrollbar_without_a_drawn_thumb_moves_the_thumb_of_its_fraction_to_the_pointer()
-> Result<(), TestError> {
    let scroll = ScrollState::new(Axis::Horizontal);
    let (row, bar) = (scroll.clone(), scroll.clone());
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.column(Modifier::new(), |ui| {
            ui.scroll(row.clone(), Modifier::new().width(200.dp()), |ui| {
                for _ in 0..4 {
                    ui.spacer(100.dp(), 40.dp(), Modifier::new());
                }
            });
            ui.scrollbar(bar.clone(), Modifier::new(), |ui, _| {
                ui.spacer(200.dp(), 10.dp(), Modifier::new());
            });
        });
    });
    let viewport = Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(125.dp(), 45.dp())));
    view.input(InputEvent::Release(Point::new(125.dp(), 45.dp())));
    assert_eq!(
        scroll.offset(),
        150.dp(),
        "the thumb of 100 dp is centered on the pointer"
    );
    Ok(())
}
