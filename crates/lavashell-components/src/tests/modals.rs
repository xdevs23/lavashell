// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn modal_scrims_take_pointer_events_without_stealing_keyboard_focus() -> Result<(), TestError> {
    let outside = State::new(0);
    let inside = State::new(0);
    let (scrim, menu) = (outside.clone(), inside.clone());
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.dialog(Modifier::new(), |ui| {
            ui.button(
                Modifier::new().focusable(false),
                scrim.action(|n| *n += 1),
                |ui| {
                    ui.spacer(200.dp(), 200.dp(), Modifier::new());
                },
            );
            ui.button(
                Modifier::new().offset(0.dp(), 50.dp()),
                menu.action(|n| *n += 1),
                |ui| {
                    ui.spacer(100.dp(), 40.dp(), Modifier::new());
                },
            );
        });
    });
    render(
        &mut view,
        Viewport::new(Size::new(200.dp(), 200.dp()), Density::ONE),
        &mut Metrics,
    )?;
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert_eq!(
        inside.get(),
        1,
        "the first real menu control receives initial focus"
    );
    click(&mut view, 150.dp(), 150.dp());
    assert_eq!(outside.get(), 1, "the scrim remains pointer interactive");
    view.input(InputEvent::Key {
        key: Key::Tab,
        shift: false,
    });
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    assert_eq!(inside.get(), 2, "Tab excludes the modal root and scrim");
    assert_eq!(outside.get(), 1);
    Ok(())
}

#[test]
fn modal_blocks_background_autofocuses_field_and_restores_keyboard_focus() -> Result<(), TestError>
{
    let open = State::new(false);
    let value = State::new(String::new());
    let clicks = State::new(0);
    let (shown, input, count) = (open.clone(), value.clone(), clicks.clone());
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.button(Modifier::new(), count.action(|n| *n += 1), |ui| {
            ui.spacer(100.dp(), 40.dp(), Modifier::new());
        });
        if shown.get() {
            ui.dialog(Modifier::new(), |ui| {
                ui.column(
                    Modifier::new()
                        .on_key({
                            let shown = shown.clone();
                            move |key| {
                                if key.key == Key::Enter {
                                    shown.set(false);
                                    true
                                } else {
                                    false
                                }
                            }
                        })
                        .offset(0.dp(), 50.dp())
                        .size(100.dp(), 40.dp()),
                    |ui| {
                        ui.text_field(
                            input.get(),
                            TextFieldOptions::default(),
                            Modifier::new(),
                            input.handler(|v, changed| *v = changed),
                        );
                    },
                );
            });
        }
    });
    let size = Size::new(200.dp(), 150.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    click(&mut view, 20.dp(), 20.dp());
    open.set(true);
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::Text("native".into()));
    assert_eq!(value.get(), "native");
    click(&mut view, 20.dp(), 20.dp());
    assert_eq!(clicks.get(), 1);
    click(&mut view, 10.dp(), 55.dp());
    view.input(InputEvent::Key {
        key: Key::Enter,
        shift: false,
    });
    assert!(!open.get());
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::Key {
        key: Key::Space,
        shift: false,
    });
    assert_eq!(clicks.get(), 2);
    Ok(())
}

#[test]
fn a_popover_opens_under_its_button_and_moves_into_the_view_by_its_size() -> Result<(), TestError> {
    let view_size = Viewport::new(Size::new(300.dp(), 200.dp()), Density::ONE);
    let popover_at = |x: Dp| -> Result<Rect, TestError> {
        let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
            let more = NodeRef::new();
            ui.stack(Modifier::new().offset(x, 30.dp()), |ui| {
                ui.spacer(20.dp(), 20.dp(), Modifier::new().node_ref(&more));
            });
            let below = more
                .bounds()
                .map(|more| Point::new(more.x, more.y + more.height));
            ui.popover(below, Modifier::new().tag("menu"), |ui| {
                ui.spacer(100.dp(), 60.dp(), Modifier::new());
            });
        });
        render(&mut view, view_size, &mut Metrics)?;
        composed_bounds(&view, "menu")
    };
    assert_eq!(
        popover_at(40.dp())?,
        Rect::new(40.dp(), 50.dp(), 100.dp(), 60.dp())
    );
    assert_eq!(
        popover_at(250.dp())?,
        Rect::new(200.dp(), 50.dp(), 100.dp(), 60.dp())
    );
    Ok(())
}

#[test]
fn a_popover_in_a_moved_parent_opens_at_its_point_in_the_view() -> Result<(), TestError> {
    let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
        let menu = NodeRef::new();
        ui.stack(Modifier::new().offset(15.dp(), 25.dp()), |ui| {
            ui.popover(
                Point::new(40.dp(), 50.dp()),
                Modifier::new().node_ref(&menu).tag("menu"),
                |ui| {
                    ui.spacer(100.dp(), 60.dp(), Modifier::new());
                },
            );
        });
        ui.spacer(
            Dp::ZERO,
            10.dp(),
            Modifier::new().width(menu.width()).tag("as wide"),
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(300.dp(), 200.dp()), Density::ONE),
        &mut Metrics,
    )?;
    assert_eq!(
        view.bounds_of("menu"),
        Some(Rect::new(40.dp(), 50.dp(), 100.dp(), 60.dp()))
    );
    assert_eq!(
        view.bounds_of("as wide").map(|bounds| bounds.width),
        Some(100.dp())
    );
    Ok(())
}
