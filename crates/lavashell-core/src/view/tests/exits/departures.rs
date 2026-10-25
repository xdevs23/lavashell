// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn an_exit_that_ends_on_the_clock_while_its_node_paints_retires_on_the_next_frame() -> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key(
                "piece",
                Modifier::new()
                    .background(PIECE)
                    .transition(fading_out(1000)),
                |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.motion().time_source(|| Duration::from_millis(1200));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 900)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1200)?;
    assert!(fills(&scene, PIECE)?.is_empty());
    assert_eq!(view.transitions.count(), 0);
    Ok(())
}

#[test]
fn a_shared_node_composed_elsewhere_keeps_its_presence_animations_shown() -> TestResult {
    let detail = State::new(false);
    let shows = detail.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let card = |ui: &mut Ui| {
            ui.shared("card", Modifier::new(), |ui| {
                match ui.animate_presence("width", 200.dp(), 40.dp(), linear(1000)) {
                    Ok(width) => well(ui, width),
                    Err(error) => ui.fail(error),
                }
            });
        };
        if shows.get() {
            ui.key("detail", Modifier::new(), card);
        } else {
            ui.key("list", Modifier::new().transition(fading_out(1000)), card);
        }
    });
    let width = |scene: &Scene| fills(scene, WELL).map(|rects| rects[0].width);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(width(&scene)?, 200.dp());
    detail.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(width(&scene)?, 200.dp());
    Ok(())
}

#[test]
fn a_child_left_behind_by_a_shared_node_that_painted_in_a_departure_fades_out() -> TestResult {
    let (listed, detail) = (State::new(true), State::new(false));
    let (shows_list, shows_detail) = (listed.clone(), detail.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if shows_list.get() {
            ui.key("list", Modifier::new().transition(fading_out(2000)), |ui| {
                ui.shared("card", Modifier::new(), |ui| {
                    ui.key(
                        "badge",
                        Modifier::new()
                            .background(PIECE)
                            .transition(fading_out(1000)),
                        |ui| {
                            ui.spacer(20.dp(), 20.dp(), Modifier::new());
                        },
                    );
                });
            });
        }
        if shows_detail.get() {
            ui.key("detail", Modifier::new(), |ui| {
                ui.shared("card", Modifier::new(), |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                });
            });
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    listed.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    detail.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    assert_near(&presences(&scene), &[0.625, 0.75]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert!(fills(&scene, PIECE)?.is_empty());
    Ok(())
}

#[test]
fn the_hover_appearance_of_a_leaving_node_continues_toward_the_last_appearance_set_in_the_tree()
-> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key(
                "button",
                Modifier::new()
                    .on_click(|| {})
                    .state_styles(half_opaque_on_hover(1000))
                    .transition(fading_out(1000)),
                |ui| {
                    ui.spacer(40.dp(), 40.dp(), Modifier::new());
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_near(&opacities(&scene), &[0.75]);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_near(&opacities(&scene), &[0.625]);
    Ok(())
}
