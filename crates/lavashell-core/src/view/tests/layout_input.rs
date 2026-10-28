// SPDX-License-Identifier: GPL-3.0-only
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(100.0), Dp::new(100.0)), Density::ONE);

#[test]
fn outlines_draw_after_content_without_expanding_layout_or_input() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.spacer(
            20.dp(),
            10.dp(),
            Modifier::new()
                .tag("outlined")
                .outline(Color::WHITE, 2.dp(), Corners::default(), 4.dp())
                .background(Color::rgb(0)),
        );
    });
    let commands = painted(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    let fill = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Fill { .. }))
        .required("a fill")?;
    let line = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Stroke { .. }))
        .required("a stroke")?;
    assert!(line > fill);
    assert_eq!(
        view.tagged_bounds("outlined")?,
        Rect::new(0.dp(), 0.dp(), 20.dp(), 10.dp())
    );
    Ok(())
}

#[test]
fn material_border_and_inset_shadow_enclose_the_finished_subtree() -> TestResult {
    const NO_OFFSET: Point = Point::new(Dp::ZERO, Dp::ZERO);
    const SHADOW: Shadow = crate::constant!(Shadow::new(Color::WHITE, Dp::new(2.0), NO_OFFSET));
    const BORDER: Stroke = crate::constant!(Stroke::new(Color::WHITE, Dp::new(1.0)));
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Axis::Vertical,
            Modifier::new().material(
                Material::new(Color::WHITE)
                    .shadow(SHADOW.inset(true))
                    .border(BORDER),
                Corners::all(8.dp()),
            ),
            |ui| {
                ui.spacer(20.dp(), 10.dp(), Modifier::new().background(Color::rgb(0)));
            },
        );
    });
    let commands = painted(view.frame(WINDOW, &mut Metrics).without_failures()?)?;
    assert!(matches!(commands.last(), Some(DrawCommand::Stroke { .. })));
    assert!(
        matches!(commands.get(commands.len() - 2), Some(DrawCommand::Shadow { shadow, .. }) if shadow.inset)
    );
    Ok(())
}

#[test]
fn modifier_order_changes_background_bounds() -> TestResult {
    let mut a = View::new(crate::platform::fixture::facts(), |ui| {
        let modifier = Modifier::new().background(Color::WHITE).padding(5.dp());
        ui.spacer(20.dp(), 10.dp(), modifier);
    });
    let mut b = View::new(crate::platform::fixture::facts(), |ui| {
        let modifier = Modifier::new().padding(5.dp()).background(Color::WHITE);
        ui.spacer(20.dp(), 10.dp(), modifier);
    });
    let rect = |cmds: Vec<DrawCommand>| {
        cmds.iter().find_map(|c| {
            if let DrawCommand::Fill { shape, .. } = c {
                Some(shape.bounds())
            } else {
                None
            }
        })
    };
    assert_eq!(
        rect(painted(a.frame(WINDOW, &mut Metrics).without_failures()?)?),
        Some(Rect::new(0.dp(), 0.dp(), 30.dp(), 20.dp()))
    );
    assert_eq!(
        rect(painted(b.frame(WINDOW, &mut Metrics).without_failures()?)?),
        Some(Rect::new(5.dp(), 5.dp(), 20.dp(), 10.dp()))
    );
    Ok(())
}

#[test]
fn padding_of_a_parent_is_outside_the_click_area_of_the_child() -> TestResult {
    let count = State::new(0);
    let c = count.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let c = c.clone();
        let click = Modifier::new().on_click(move || c.update(|n| *n += 1));
        ui.layout(
            Arrangement::Overlay,
            Modifier::new().padding(5.dp()),
            |ui| {
                ui.spacer(20.dp(), 10.dp(), click);
            },
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Press(Point {
        x: 2.dp(),
        y: 2.dp(),
    }));
    view.input(InputEvent::Release(Point {
        x: 2.dp(),
        y: 2.dp(),
    }));
    assert_eq!(count.get(), 0);
    view.input(InputEvent::Press(Point {
        x: 7.dp(),
        y: 7.dp(),
    }));
    view.input(InputEvent::Release(Point {
        x: 7.dp(),
        y: 7.dp(),
    }));
    assert_eq!(count.get(), 1);
    Ok(())
}

#[test]
fn weighted_children_share_remaining_width() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let row = Modifier::new().fill_max_width().gap(5.dp());
        ui.layout(Axis::Horizontal, row, |ui| {
            ui.spacer(20.dp(), 10.dp(), Modifier::new());
            ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(1.0).tag("flex"));
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(view.tagged_bounds("flex")?.width, 75.dp());
    Ok(())
}

#[test]
fn clip_excludes_input_and_keyboard_activates() -> TestResult {
    let count = State::new(0);
    let c = count.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let c = c.clone();
        let modifier = Modifier::new().on_click(move || c.update(|n| *n += 1));
        ui.spacer(30.dp(), 30.dp(), modifier.clip(Corners::all(15.dp())));
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Press(Point {
        x: 1.dp(),
        y: 1.dp(),
    }));
    view.input(InputEvent::Release(Point {
        x: 1.dp(),
        y: 1.dp(),
    }));
    assert_eq!(count.get(), 0);
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(count.get(), 1);
    Ok(())
}

#[test]
fn keyed_focus_survives_reordering() -> TestResult {
    let order = State::new(vec![1, 2]);
    let o = order.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            for key in o.get() {
                ui.key(key, Modifier::new(), |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new().on_click(|| {}));
                });
            }
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    view.input(InputEvent::Press(Point {
        x: 10.dp(),
        y: 10.dp(),
    }));
    let focus = view.focus();
    order.set(vec![2, 1]);
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(view.focus(), focus);
    Ok(())
}

#[test]
fn weighted_children_share_remaining_height() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let column = Modifier::new().gap(5.dp()).fill_max_height();
        ui.layout(Axis::Vertical, column, |ui| {
            ui.spacer(10.dp(), 20.dp(), Modifier::new());
            ui.spacer(10.dp(), 0.dp(), Modifier::new().weight(1.0).tag("flex"));
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    assert_eq!(
        view.tagged_bounds("flex")?,
        Rect::new(0.dp(), 25.dp(), 10.dp(), 75.dp())
    );
    Ok(())
}

#[test]
fn maximum_width_preserves_short_text_and_bounds_long_text_before_its_sibling() -> TestResult {
    let value = State::new("short".to_owned());
    let text = value.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new().gap(8.dp()), |ui| {
            let bounded = Modifier::new().font_size(10.sp()).tag("bounded");
            ui.text(text.get(), bounded.max_width(100.dp()));
            ui.spacer(10.dp(), 10.dp(), Modifier::new().tag("sibling"));
        });
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(view.tagged_bounds("bounded")?.width, 25.dp());
    assert_eq!(view.tagged_bounds("sibling")?.x, 33.dp());
    value.set("long title ".repeat(10));
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(300.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(view.tagged_bounds("bounded")?.width, 100.dp());
    assert_eq!(view.tagged_bounds("sibling")?.x, 108.dp());
    Ok(())
}

#[test]
fn local_clip_viewport_preserves_natural_measurement_and_updates_paint_and_input_together()
-> TestResult {
    let extent = State::new(30.dp());
    let measured = State::new(Size::ZERO);
    let count = State::new(0);
    let source = extent.clone();
    let observed = measured.clone();
    let activated = count.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let observed = observed.clone();
        let activated = activated.clone();
        let clip = Rect::new(0.dp(), 0.dp(), 100.dp(), source.get());
        let viewport = Modifier::new()
            .offset(20.dp(), 10.dp())
            .clip_rect(clip, Corners::all(8.dp()));
        let content = Modifier::new()
            .on_measure(move |size| observed.set(size))
            .on_click(move || activated.update(|n| *n += 1));
        ui.layout(Arrangement::Overlay, viewport, |ui| {
            ui.spacer(100.dp(), 100.dp(), content);
        });
    });
    let mut scene = Scene::default();
    let frame = view.frame(
        Viewport::new(Size::new(200.dp(), 200.dp()), Density::ONE),
        &mut Metrics,
    );
    scene.show(frame.without_failures()?)?;
    assert_eq!(measured.get(), Size::new(100.dp(), 100.dp()));
    let click = |view: &mut View, point| {
        view.input(InputEvent::Press(point));
        view.input(InputEvent::Release(point));
    };
    click(&mut view, Point::new(30.dp(), 60.dp()));
    assert_eq!(count.get(), 0);
    click(&mut view, Point::new(30.dp(), 20.dp()));
    assert_eq!(count.get(), 1);
    extent.set(80.dp());
    let frame = view.frame(
        Viewport::new(Size::new(200.dp(), 200.dp()), Density::ONE),
        &mut Metrics,
    );
    scene.show(frame.without_failures()?)?;
    let commands = scene.commands();
    assert!(commands.iter().any(|command| matches!(command,
        DrawCommand::Clip { rect, corners } if *rect == Rect::new(20.dp(), 10.dp(), 100.dp(), 80.dp())
            && *corners == Corners::all(8.dp()).into())));
    assert_eq!(measured.get(), Size::new(100.dp(), 100.dp()));
    click(&mut view, Point::new(30.dp(), 60.dp()));
    assert_eq!(count.get(), 2);
    Ok(())
}

#[test]
fn out_of_flow_content_keeps_natural_bounds_without_resizing_or_spacing_siblings() -> TestResult {
    let arrangement = State::new(Arrangement::Overlay);
    let source = arrangement.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let decoration = Modifier::new()
            .out_of_flow()
            .enabled(false)
            .tag("decoration")
            .background(Color::WHITE);
        ui.layout(
            source.get(),
            Modifier::new().gap(7.dp()).tag("container"),
            |ui| {
                ui.spacer(40.dp(), 30.dp(), Modifier::new().tag("first"));
                ui.spacer(120.dp(), 90.dp(), decoration);
                ui.spacer(20.dp(), 10.dp(), Modifier::new().tag("second"));
            },
        );
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(400.dp(), 400.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.tagged_bounds("container")?,
        Rect::new(0.dp(), 0.dp(), 40.dp(), 30.dp())
    );
    assert_eq!(
        view.tagged_bounds("decoration")?,
        Rect::new(0.dp(), 0.dp(), 120.dp(), 90.dp())
    );
    arrangement.set(Axis::Vertical.into());
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(400.dp(), 400.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.tagged_bounds("container")?,
        Rect::new(0.dp(), 0.dp(), 40.dp(), 47.dp())
    );
    assert_eq!(view.tagged_bounds("second")?.y, 37.dp());
    assert_eq!(
        view.tagged_bounds("decoration")?,
        Rect::new(0.dp(), 0.dp(), 120.dp(), 90.dp())
    );
    arrangement.set(Axis::Horizontal.into());
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(400.dp(), 400.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.tagged_bounds("container")?,
        Rect::new(0.dp(), 0.dp(), 67.dp(), 30.dp())
    );
    assert_eq!(view.tagged_bounds("second")?.x, 47.dp());
    Ok(())
}

#[test]
fn bounded_view_can_measure_natural_content_before_resize_while_clipping_input_to_its_allocation()
-> TestResult {
    let measured = State::new(Size::ZERO);
    let observed = measured.clone();
    let clicked = State::new(false);
    let hit = clicked.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let viewport = ui.viewport();
        let observed = observed.clone();
        let hit = hit.clone();
        ui.layout(
            Arrangement::Overlay,
            Modifier::new()
                .tag("allocated")
                .measurement_limit(Size::new(320.dp(), 1080.dp()))
                .clip_rect(
                    Rect::new(0.dp(), 0.dp(), viewport.size.width, viewport.size.height),
                    Corners::default(),
                ),
            |ui| {
                ui.layout(
                    Axis::Vertical,
                    Modifier::new()
                        .on_measure(move |size| {
                            observed.set_if_changed(size);
                        })
                        .padding(12.dp()),
                    |ui| {
                        ui.spacer(
                            296.dp(),
                            200.dp(),
                            Modifier::new()
                                .tag("natural-child")
                                .on_click(move || hit.set(true)),
                        );
                    },
                );
            },
        );
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(Size::new(320.dp(), 1.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(measured.get(), Size::new(320.dp(), 224.dp()));
    assert_eq!(view.tagged_bounds("allocated")?.height, 1.dp());
    assert_eq!(view.tagged_bounds("natural-child")?.height, 200.dp());
    let point = Point::new(20.dp(), 120.dp());
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    assert!(!clicked.get());
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(measured.get(), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(view.tagged_bounds("allocated")?.height, 224.dp());
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Release(point));
    assert!(clicked.get());
    Ok(())
}
