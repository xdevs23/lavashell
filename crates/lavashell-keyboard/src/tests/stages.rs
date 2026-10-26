// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::look;

fn panel() -> Rect {
    let height = PANEL.get();
    Rect::new(Dp::ZERO, SCREEN.height - height, SCREEN.width, height)
}

fn empty() -> TextSession {
    session(
        SessionId(1),
        TextHints::default(),
        FieldText::BeforeCursor("".into()),
    )
}

#[test]
fn the_glass_grows_first_then_the_rows_show_fused_then_the_keys_split() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.session.set(Some(empty()));
    host.frame()?;
    run_until(&mut host.view, &mut host.scene, |_, scene| {
        assert!(unions(scene, 10).is_empty(), "no key shows before the rows");
        !fused_rows(scene).is_empty()
    })?;
    let glass = backdrops(&host.scene)[0];
    assert!(
        glass.y <= panel().y + 1.dp(),
        "the rows show once the glass is within a pixel of its height: {glass:?}"
    );
    assert_eq!(fused_rows(&host.scene).len(), 4, "each row is one shape");
    run_until(&mut host.view, &mut host.scene, |_, scene| {
        !unions(scene, 10).is_empty()
    })?;
    host.settle()?;
    assert_eq!(unions(&host.scene, 10).len(), 1, "the keys split apart");
    assert!(fused_rows(&host.scene).is_empty());
    Ok(())
}

#[test]
fn the_keys_fuse_into_rows_before_the_panel_sinks() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.start(TextHints::default(), "")?;
    let rest = node(&host.view, &Role::Group, "Keyboard")?.bounds;
    host.session.set(None);
    host.frame()?;
    run_until(&mut host.view, &mut host.scene, |_, scene| {
        fused_rows(scene).len() == 4
    })?;
    let fused = fused_rows(&host.scene);
    assert_eq!(node(&host.view, &Role::Group, "Keyboard")?.bounds, rest);
    assert!(!has(&host.view, &Role::Button, "q"));
    run_until(&mut host.view, &mut host.scene, |view, _| {
        !has(view, &Role::Group, "Keyboard")
    })?;
    run_until(&mut host.view, &mut host.scene, |_, scene| {
        fused_rows(scene)
            .first()
            .is_some_and(|row| row.y > fused[0].y)
    })?;
    host.settle()?;
    assert!(fused_rows(&host.scene).is_empty());
    Ok(())
}

#[test]
fn under_reduced_motion_the_keyboard_shows_and_hides_whole_at_once() -> Result<(), TestError> {
    let mut host = Host::new(settings())?;
    host.view.provide(&REDUCED_MOTION, true);
    host.session.set(Some(empty()));
    host.frame()?;
    assert_eq!(node(&host.view, &Role::Group, "Keyboard")?.bounds, panel());
    assert_eq!(unions(&host.scene, 10).len(), 1);
    host.session.set(None);
    host.frame()?;
    assert!(!host.keyboard.shown());
    assert!(backdrops(&host.scene).is_empty());
    Ok(())
}

/// A view and the keyboard composed in it. The returned state sets the text session of the field
/// edited by the host.
pub(super) type Taskbar = (View, State<Option<TextSession>>, Keyboard);

/// A view in which the host shows a taskbar island, shared under the key `taskbar`, while the
/// keyboard is hidden. The keyboard at `placement` grows out of that island.
pub(super) fn with_taskbar(placement: State<Placement>) -> Result<Taskbar, TestError> {
    let field = State::new(None);
    let target = Target::Host {
        session: field.clone(),
        sink: Rc::new(|_| {}),
    };
    let settings = State::new(settings());
    let keyboard = Keyboard::new(target, &settings, State::new(&ENGLISH), placement)?;
    let composed = keyboard.clone();
    let top = 390.dp();
    let view = View::new(lavashell_test_platform::facts(), move |ui| {
        ui.layout(Arrangement::Overlay, Modifier::new(), |ui| {
            if !composed.shown() {
                let blur = constant!(Backdrop::blur(Dp::new(3.0)));
                let glass = Material::new(Color::WHITE).backdrop(blur);
                ui.shared(
                    "taskbar",
                    Modifier::new()
                        .tag("taskbar")
                        .offset(12.dp(), top)
                        .size(1013.dp(), 64.dp())
                        .material(glass, 32.dp())
                        .transition(Transition::new(look::hide_spring()).reflow()),
                    |_| {},
                );
            }
            above_keyboard(
                ui,
                |_| {},
                |ui| {
                    ui.keyboard_from(&composed, "taskbar", SCREEN.height - top, Modifier::new());
                },
            );
        });
    });
    Ok((view, field, keyboard))
}

#[test]
fn a_hidden_keyboard_from_an_island_is_as_tall_as_the_island_lies_above_its_bottom()
-> Result<(), TestError> {
    let (mut view, _field, keyboard) = with_taskbar(State::new(Placement::FullWidth))?;
    let mut scene = Scene::default();
    frame(&mut view, &mut scene, 0)?;
    let taskbar = composed_bounds(&view, "taskbar")?;
    assert_eq!(keyboard.height(), SCREEN.height - taskbar.y);
    Ok(())
}

#[test]
fn a_composition_gets_the_height_of_a_hidden_keyboard_from_an_island_in_its_first_frame()
-> Result<(), TestError> {
    let target = Target::Host {
        session: State::new(None),
        sink: Rc::new(|_| {}),
    };
    let settings = State::new(settings());
    let placement = State::new(Placement::FullWidth);
    let keyboard = Keyboard::new(target, &settings, State::new(&ENGLISH), placement)?;
    let (read, closed) = (Rc::new(Cell::new(Dp::ZERO)), 76.dp());
    let recorded = read.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        recorded.set(keyboard.height());
        ui.keyboard_from(&keyboard, "taskbar", closed, Modifier::new());
    });
    frame(&mut view, &mut Scene::default(), 0)?;
    assert_eq!(read.get(), closed);
    Ok(())
}

#[test]
fn the_glass_grows_out_of_the_shared_node_of_the_host_and_shrinks_back_into_it()
-> Result<(), TestError> {
    let (mut view, field, _) = with_taskbar(State::new(Placement::FullWidth))?;
    let mut scene = Scene::default();
    frame(&mut view, &mut scene, 0)?;
    let taskbar = composed_bounds(&view, "taskbar")?;
    field.set(Some(empty()));
    frame(&mut view, &mut scene, 0)?;
    assert_eq!(view.bounds_of("taskbar"), None);
    assert!(
        near(backdrops(&scene)[0], taskbar),
        "it starts as the taskbar"
    );
    settle(&mut view, &mut scene)?;
    assert!(near(backdrops(&scene)[0], panel()));
    field.set(None);
    refresh(&mut view, &mut scene)?;
    run_until(&mut view, &mut scene, |view, scene| {
        let back = view.bounds_of("taskbar").is_some();
        assert!(
            back || near(backdrops(scene)[0], panel()),
            "the keys fuse first"
        );
        back
    })?;
    let returning = composed_bounds(&view, "taskbar")?;
    assert!(near(returning, panel()), "it starts as the panel");
    settle(&mut view, &mut scene)?;
    assert!(near(composed_bounds(&view, "taskbar")?, taskbar));
    Ok(())
}

#[test]
fn the_content_above_ends_at_the_glass_in_every_frame_while_it_grows_out_of_the_island_and_back()
-> Result<(), TestError> {
    let (mut view, field, _) = with_taskbar(State::new(Placement::FullWidth))?;
    let mut scene = Scene::default();
    frame(&mut view, &mut scene, 0)?;
    assert!(ends_at_glass(&view, &scene), "at rest in the taskbar");
    field.set(Some(empty()));
    refresh(&mut view, &mut scene)?;
    run_until(&mut view, &mut scene, |view, scene| {
        assert!(ends_at_glass(view, scene), "while the glass grows");
        !view.motion().is_active()
    })?;
    field.set(None);
    refresh(&mut view, &mut scene)?;
    run_until(&mut view, &mut scene, |view, scene| {
        assert!(ends_at_glass(view, scene), "after the field is gone");
        !view.motion().is_active()
    })?;
    Ok(())
}

#[test]
fn the_content_of_an_island_shows_again_once_the_glass_is_back_but_one_pixel() {
    let spring = look::hide_spring();
    let pixel = 1.dp();
    let start = Vector([300.0]);
    let rest = Vector([0.0]);
    let left = |time: Duration| spring.sample(&start, &rest, time.as_secs_f64()).0.0[0];
    let time = look::hide_time(300.dp(), pixel);
    assert!(left(time) <= pixel.get(), "back but one pixel");
    assert!(
        left(time.saturating_sub(Duration::from_millis(1))) > pixel.get(),
        "not a millisecond before"
    );
    assert_eq!(look::hide_time(pixel, pixel), Duration::ZERO);
    assert!(
        look::hide_time(600.dp(), pixel) > time,
        "a longer way takes longer"
    );
    assert!(
        look::hide_time(300.dp(), 0.5.dp()) > time,
        "a finer pixel takes longer"
    );
}

#[test]
fn the_return_to_an_island_waits_for_one_pixel_of_the_view() -> Result<(), TestError> {
    let after = |scale: f64| -> Result<Duration, TestError> {
        let content_after = Rc::new(Cell::new(Duration::ZERO));
        let seen = Rc::clone(&content_after);
        let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
            let distance = constant!(NonNegativeDp::try_new(Dp::new(300.0)));
            let island = IslandReturn::new(ui, distance);
            seen.set(island.content_after);
        });
        let density = Density::from_scale(scale).ok_or(TestError::Density(scale))?;
        let viewport = Viewport::new(SCREEN, density);
        let mut tree = SceneTree::default();
        let timing = FrameTime::default();
        view.show_frame_at(&mut tree, viewport, &mut Metrics, timing)?;
        Ok(content_after.get())
    };
    assert_eq!(after(1.0)?, look::hide_time(300.dp(), 1.dp()));
    assert!(after(2.0)? > after(1.0)?, "a finer pixel takes longer");
    Ok(())
}

#[test]
fn a_keyboard_from_an_island_covers_nothing_until_it_rises_and_rests_hidden_once_its_glass_is_back()
-> Result<(), TestError> {
    let (mut view, field, keyboard) = with_taskbar(State::new(Placement::FullWidth))?;
    let mut scene = Scene::default();
    frame(&mut view, &mut scene, 0)?;
    assert!(keyboard.rests_hidden());
    assert_eq!(keyboard.covered(), Dp::ZERO);
    field.set(Some(empty()));
    frame(&mut view, &mut scene, 0)?;
    assert!(!keyboard.rests_hidden());
    assert!(
        keyboard.covered() < panel().height,
        "the panel has not risen yet"
    );
    settle(&mut view, &mut scene)?;
    assert!((keyboard.covered() - panel().height).abs() < 0.5.dp());
    field.set(None);
    refresh(&mut view, &mut scene)?;
    run_until(&mut view, &mut scene, |_, _| !keyboard.shown())?;
    assert!(
        !keyboard.rests_hidden(),
        "the glass is still on its way back"
    );
    settle(&mut view, &mut scene)?;
    assert!(keyboard.rests_hidden());
    assert_eq!(keyboard.covered(), Dp::ZERO);
    Ok(())
}
