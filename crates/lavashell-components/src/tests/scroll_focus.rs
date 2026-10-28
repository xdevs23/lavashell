// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn focused_keys_bubble_and_modifiers_reach_recycled_items() -> Result<(), TestError> {
    let selected = State::new(None);
    let keys = State::new(0);
    let result = selected.clone();
    let counter = keys.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.column(
            Modifier::new().on_key({
                let counter = counter.clone();
                move |shortcut: Shortcut| {
                    counter.update(|n| *n += 1);
                    shortcut.key == Key::Down
                }
            }),
            |ui| {
                let result = result.clone();
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new().on_activate(move |event| result.set(Some(event.modifiers))),
                );
            },
        );
    });
    render(
        &mut view,
        Viewport::new(Size::new(200.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
    )?;
    view.input(InputEvent::Modifiers(InputModifiers {
        control: true,
        shift: true,
        alt: false,
    }));
    click(&mut view, 20.dp(), 20.dp());
    let modifiers = selected
        .get()
        .ok_or(TestError::NothingRecorded("activation"))?;
    assert_eq!(
        modifiers,
        InputModifiers {
            control: true,
            shift: true,
            alt: false
        }
    );
    view.input(InputEvent::Key {
        key: Key::Down,
        shift: false,
    });
    assert_eq!(keys.get(), 1);
    Ok(())
}

#[test]
fn animated_scroll_uses_presentation_time_and_recycling_remains_bounded() -> Result<(), TestError> {
    use std::time::Duration;
    let spring = Spring::new(Duration::from_millis(170), 1.0)?;
    let recycler = Recycler::<usize>::new();
    let list = recycler.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        if let Err(error) = list.scroll.smooth(ui.motion(), spring) {
            ui.fail(error);
        }
        list.compose(
            ui,
            RecyclerItems {
                count: 10_000,
                key: |i| i,
                view_type: |_| 0,
            },
            RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(item.size.width, item.size.height, Modifier::new());
            },
        );
    });
    let frame = |ms| FrameTime {
        sequence: ms,
        delta: Duration::ZERO,
        now: Duration::from_millis(ms),
        presentation: Duration::from_millis(ms),
        refresh: Some(Duration::from_nanos(8_333_333)),
    };
    let size = Size::new(500.dp(), 400.dp());
    let viewport = Viewport::new(size, Density::ONE);
    let mut tree = SceneTree::default();
    (view.show_frame_at(&mut tree, viewport, &mut Metrics, frame(0))).without_failures()?;
    view.input(InputEvent::Scroll {
        at: Point::new(20.dp(), 20.dp()),
        delta: Point::new(Dp::ZERO, 300.dp()),
    });
    assert!(view.needs_frame());
    let mut previous = Dp::ZERO;
    for ms in [8, 16, 33, 80, 160, 400, 900] {
        (view.show_frame_at(&mut tree, viewport, &mut Metrics, frame(ms))).without_failures()?;
        let offset = recycler.scroll.offset();
        assert!(offset >= previous && offset <= 300.dp());
        assert!(recycler.stats().holders <= 3 * 11);
        previous = offset;
    }
    assert_eq!(previous, 300.dp());
    Ok(())
}

#[test]
fn scrollbar_grip_survives_recomposition_during_pointer_capture() -> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let list = recycler.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.row(Modifier::new(), |ui| {
            list.compose(
                ui,
                RecyclerItems {
                    count: 100,
                    key: |i| i,
                    view_type: |_| 0,
                },
                RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
                Modifier::new().size(180.dp(), 400.dp()),
                |ui, item| {
                    ui.spacer(item.size.width, item.size.height, Modifier::new());
                },
            );
            ui.scrollbar(list.scroll.clone(), Modifier::new(), |ui, bar| {
                ui.spacer(20.dp(), 400.dp(), Modifier::new());
                ui.spacer(20.dp(), bar.thumb_length(400.dp()), bar.follow(400.dp()));
            });
        });
    });
    let size = Size::new(200.dp(), 400.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    view.input(InputEvent::Press(Point::new(190.dp(), 10.dp())));
    view.input(InputEvent::Move(Point::new(190.dp(), 100.dp())));
    render(&mut view, viewport, &mut Metrics)?;
    let first = recycler.scroll.offset();
    view.input(InputEvent::Move(Point::new(190.dp(), 110.dp())));
    render(&mut view, viewport, &mut Metrics)?;
    let second = recycler.scroll.offset();
    assert!(
        is_close(second.get(), (first + 100.dp()).get()),
        "drag must retain the original grip across frames"
    );
    view.input(InputEvent::Release(Point::new(190.dp(), 500.dp())));
    assert_eq!(recycler.scroll.offset(), 3600.dp());
    Ok(())
}

#[test]
fn large_selection_snapshots_share_storage_and_mutations_are_isolated() {
    let mut selection = Selection::new();
    selection.select_all(&(0..100_000).collect::<Vec<_>>());
    let snapshot = selection.clone();
    assert!(
        std::ptr::eq(selection.keys(), snapshot.keys()),
        "reading a large selection must not copy every item"
    );
    selection.select(1, InputModifiers::default(), &[0, 1, 2]);
    assert_eq!(selection.keys().len(), 1);
    assert_eq!(snapshot.keys().len(), 100_000);
}

#[test]
fn keyboard_focus_stays_in_the_list_when_a_focused_holder_is_rebound() -> Result<(), TestError> {
    for scoped in [false, true] {
        focus_after_rebinding(scoped)?;
    }
    Ok(())
}

fn focus_after_rebinding(scoped: bool) -> Result<(), TestError> {
    let list = Recycler::<usize>::new();
    let recycler = list.clone();
    let handled = State::new(0);
    let events = handled.clone();
    let search = State::new(String::new());
    let field = search.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.column(Modifier::new(), |ui| {
            ui.text_field(
                field.get(),
                TextFieldOptions::default(),
                Modifier::new(),
                field.handler(|text, value| *text = value),
            );
            let recycler = recycler.clone();
            let events = events.clone();
            let compose = move |ui: &mut Ui| {
                recycler.compose(
                    ui,
                    RecyclerItems {
                        count: 10_000,
                        key: |i| i,
                        view_type: |_| 0,
                    },
                    RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
                    Modifier::new()
                        .on_key({
                            let scroll = recycler.scroll.clone();
                            let events = events.clone();
                            move |key| {
                                match key.key {
                                    Key::End => scroll.jump_to(scroll.content().height),
                                    Key::Home => scroll.jump_to(0.dp()),
                                    Key::Character('a') if key.control => {}
                                    _ => return false,
                                }
                                events.update(|n| *n += 1);
                                true
                            }
                        })
                        .fill_max_width()
                        .height(360.dp()),
                    |ui, item| {
                        ui.spacer(
                            item.size.width,
                            item.size.height,
                            Modifier::new().on_click(|| {}),
                        );
                    },
                );
            };
            if scoped {
                ui.scoped("recycler", Modifier::new(), compose);
            } else {
                compose(ui);
            }
        });
    });
    let size = Size::new(500.dp(), 400.dp());
    let viewport = Viewport::new(size, Density::ONE);
    render(&mut view, viewport, &mut Metrics)?;
    click(&mut view, 20.dp(), 50.dp());
    for key in [
        Shortcut::key(Key::End),
        Shortcut::key(Key::Home),
        Shortcut::control(Key::Character('a')),
    ] {
        view.input(InputEvent::Shortcut(key));
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert_eq!(
        handled.get(),
        3,
        "focus must not jump to the search field after End"
    );
    assert_eq!(list.scroll.offset(), 0.dp());
    assert!(search.get().is_empty());
    Ok(())
}
