// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::{cell::RefCell, rc::Rc};

#[test]
fn a_key_composed_again_during_its_exit_returns_from_the_values_the_exit_reached() -> TestResult {
    let shown = State::new(true);
    let mut scene = Scene::default();
    let mut view = presence_well(&shown);
    let width = |scene: &Scene| fills(scene, WELL).map(|rects| rects[0].width);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(width(&scene)?, 120.dp());
    assert_near(&presences(&scene), &[0.5]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 2000)?;
    assert_eq!(width(&scene)?, 160.dp());
    assert_near(&presences(&scene), &[0.75]);
    Ok(())
}

#[test]
fn a_key_that_comes_back_during_an_exit_has_the_state_remembered_for_it() -> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let remembered: Rc<RefCell<Option<State<u32>>>> = Rc::default();
    let record = remembered.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key("item", Modifier::new().transition(fading_out(1000)), |ui| {
                *record.borrow_mut() = Some(ui.state("taps", || 0));
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            });
        }
    });
    let taps = || -> Result<State<u32>, TestError> {
        let remembered = remembered.borrow().clone();
        remembered.required("the state remembered for the item")
    };
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    taps()?.set(3);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(taps()?.get(), 3);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(taps()?.get(), 0);
    Ok(())
}

#[test]
fn a_component_painted_in_a_leaving_subtree_paints_the_live_appearance_when_its_key_comes_back()
-> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let present = present.clone();
        ui.scoped("list", Modifier::new(), move |ui| {
            if present.get() {
                ui.key("item", Modifier::new().transition(fading_out(4000)), |ui| {
                    ui.scoped("button", Modifier::new(), |ui| {
                        ui.key(
                            "face",
                            Modifier::new()
                                .on_click(|| {})
                                .state_styles(half_opaque_on_hover(100)),
                            |ui| {
                                ui.spacer(40.dp(), 40.dp(), Modifier::new());
                            },
                        );
                    });
                });
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::Move(Point::new(20.dp(), 20.dp())));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    assert_near(&opacities(&scene), &[0.5]);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 100)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 200)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 200)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    assert_near(&opacities(&scene), &[0.75]);
    Ok(())
}

#[test]
fn a_child_dropped_in_the_composition_that_brings_its_leaving_parent_back_starts_its_exit()
-> TestResult {
    let (child_shown, parent_shown) = (State::new(true), State::new(true));
    let mut scene = Scene::default();
    let mut view = parent_and_child(&parent_shown, &child_shown, 1000, 1000);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    parent_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    parent_shown.set(true);
    child_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    assert_near(&presences(&scene), &[0.625, 0.75]);
    Ok(())
}
