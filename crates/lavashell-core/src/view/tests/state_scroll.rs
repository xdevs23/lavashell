// SPDX-License-Identifier: GPL-3.0-only
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

#[test]
fn keyed_state_survives_reorder_and_drops_after_removal() -> TestResult {
    let order = State::new(vec![1, 2]);
    let initializations = std::rc::Rc::new(std::cell::Cell::new(0));
    let sequence = order.clone();
    let initialized = initializations.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            for key in sequence.get() {
                ui.key(key, Modifier::new(), |ui| {
                    let count = ui.state("count", || {
                        initialized.set(initialized.get() + 1);
                        0
                    });
                    ui.text(
                        count.get().to_string(),
                        Modifier::new().tag(format!("counter-{key}")),
                    );
                    ui.spacer(
                        20.dp(),
                        20.dp(),
                        Modifier::new()
                            .tag(format!("increment-{key}"))
                            .on_click(move || count.update(|n| *n += 1)),
                    );
                });
            }
        });
    });
    let window = Viewport::new(Size::new(100.dp(), 200.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    let rect = view.tagged_bounds("increment-1")?;
    let point = Point {
        x: rect.x + 5.dp(),
        y: rect.y + 5.dp(),
    };
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    order.set(vec![2, 1]);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    assert_eq!(initializations.get(), 2);
    let index = view
        .nodes
        .iter()
        .position(|n| n.tag.as_deref() == Some("counter-1"))
        .required("a node tagged counter-1")?;
    assert!(matches!(&view.nodes[index].kind, NodeKind::Text(s, _) if s == "1"));
    order.set(vec![2]);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    assert_eq!(view.memory.borrow().len(), 1);
    order.set(vec![2, 1]);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    assert_eq!(initializations.get(), 3);
    let index = view
        .nodes
        .iter()
        .position(|n| n.tag.as_deref() == Some("counter-1"))
        .required("a node tagged counter-1")?;
    assert!(matches!(&view.nodes[index].kind, NodeKind::Text(s, _) if s == "0"));
    Ok(())
}

#[test]
fn paint_state_updates_without_recomposition_or_measurement() -> TestResult {
    let value = State::new(10.dp());
    let compositions = std::rc::Rc::new(std::cell::Cell::new(0));
    let composed = compositions.clone();
    let state = value.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        composed.set(composed.get() + 1);
        let state = state.clone();
        ui.canvas(
            Size::new(100.dp(), 20.dp()),
            Modifier::new(),
            move |drawing| {
                drawing.fill(
                    Rect::new(0.dp(), 0.dp(), state.get(), 20.dp()),
                    Color::WHITE,
                );
            },
        );
    });
    let window = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    let mut scene = Scene::default();
    scene.show(view.frame(window, &mut Metrics)?)?;
    assert!(!view.needs_frame());
    value.set(40.dp());
    assert!(view.needs_frame());
    assert!(!view.scope.dirty().contains(Phase::Layout));
    scene.show(view.frame(window, &mut Metrics)?)?;
    assert!(
        scene.commands().iter().any(
            |c| matches!(c, DrawCommand::Fill { shape, .. } if shape.bounds().width == 40.dp())
        )
    );
    assert_eq!(compositions.get(), 1);
    Ok(())
}

#[test]
fn viewport_resize_recomposes_responsive_content() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.spacer(
            if ui.viewport().size.width < 80.dp() {
                20.dp()
            } else {
                40.dp()
            },
            10.dp(),
            Modifier::new().tag("responsive"),
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(view.tagged_bounds("responsive")?.width, 40.dp());
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(60.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(view.tagged_bounds("responsive")?.width, 20.dp());
    Ok(())
}

#[test]
fn dragging_keeps_capture_outside_the_target() -> TestResult {
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = events.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let captured = captured.clone();
        ui.spacer(
            40.dp(),
            20.dp(),
            Modifier::new()
                .on_pointer(move |p| captured.borrow_mut().push(p))
                .offset(10.dp(), 10.dp()),
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Press(Point {
        x: 15.dp(),
        y: 15.dp(),
    }));
    view.input(InputEvent::Move(Point {
        x: 90.dp(),
        y: 15.dp(),
    }));
    view.input(InputEvent::Release(Point {
        x: 90.dp(),
        y: 15.dp(),
    }));
    view.input(InputEvent::Move(Point {
        x: 20.dp(),
        y: 20.dp(),
    }));
    let events = events.borrow();
    assert_eq!(events.len(), 3);
    assert_eq!(events[1].position.x, 80.dp());
    assert_eq!(events[1].phase, PointerPhase::Moved);
    assert_eq!(events[2].phase, PointerPhase::Released);
    assert_eq!(events[2].size, Size::new(40.dp(), 20.dp()));
    Ok(())
}

#[test]
fn text_events_accumulate_before_a_frame_and_backspace_handles_unicode() -> TestResult {
    let value = State::new(String::new());
    let controlled = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let changed = controlled.clone();
        ui.text_input(
            controlled.get(),
            TextFieldOptions::default(),
            Modifier::new(),
            move |s| changed.set(s),
        );
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Text("a".into()));
    view.input(InputEvent::Text("é".into()));
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    assert_eq!(value.get(), "aé ");
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Backspace,
        shift: false,
    });
    assert_eq!(value.get(), "a");
    Ok(())
}

#[test]
fn scrolling_clamps_offset_and_clips_activation() -> TestResult {
    let activations = State::new(0);
    let counter = activations.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(Axis::Vertical, Modifier::new().height(60.dp()), |ui| {
            for i in 0..4 {
                let counter = counter.clone();
                ui.spacer(
                    40.dp(),
                    30.dp(),
                    Modifier::new()
                        .tag(format!("item-{i}"))
                        .on_click(move || counter.update(|n| *n += 1)),
                );
            }
        });
    });
    let window = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    let outside = Point {
        x: 10.dp(),
        y: 75.dp(),
    };
    view.input(InputEvent::Press(outside));
    view.input(InputEvent::Release(outside));
    assert_eq!(activations.get(), 0);
    view.input(InputEvent::Scroll {
        at: Point {
            x: 10.dp(),
            y: 20.dp(),
        },
        delta: Point::new(Dp::ZERO, 1000.dp()),
    });
    advance_with(&mut scene, &mut view, window, &mut Metrics)?;
    assert_eq!(view.tagged_bounds("item-2")?.y, 0.dp());
    view.input(InputEvent::Press(Point {
        x: 10.dp(),
        y: 15.dp(),
    }));
    view.input(InputEvent::Release(Point {
        x: 10.dp(),
        y: 15.dp(),
    }));
    assert_eq!(activations.get(), 1);
    Ok(())
}
