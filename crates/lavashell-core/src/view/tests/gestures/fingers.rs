// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn two_fingers_moving_together_on_a_list_scroll_it_as_far_as_one_finger() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), State::new(0), Rc::default())?;
    let tab = center(view.tagged_bounds("tab 0")?);
    let (first, second) = (
        tab.offset(-50.dp(), Dp::ZERO),
        tab.offset(50.dp(), Dp::ZERO),
    );
    let finger = |point, at| TouchSample {
        id: touch(point),
        at,
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(finger(1, first)));
    view.input(InputEvent::TouchDown(finger(2, second)));
    for (index, lift) in [-30.dp(), -50.dp()].into_iter().enumerate() {
        view.input(InputEvent::TouchMove(finger(
            1,
            first.offset(Dp::ZERO, lift),
        )));
        advance(
            &mut scene,
            &mut view,
            WINDOW_SIZE,
            16 * (2 * index as u64 + 1),
        )?;
        view.input(InputEvent::TouchMove(finger(
            2,
            second.offset(Dp::ZERO, lift),
        )));
        advance(
            &mut scene,
            &mut view,
            WINDOW_SIZE,
            16 * (2 * index as u64 + 2),
        )?;
    }
    assert_eq!(scroll.offset(), 42.dp());
    Ok(())
}

#[test]
fn a_tab_removed_before_the_finger_moves_leaves_the_finger_to_the_list() -> TestResult {
    let (scroll, clicks, shown) = (
        ScrollState::new(Axis::Vertical),
        State::new(0),
        State::new(true),
    );
    let (state, counter, show) = (scroll.clone(), clicks.clone(), shown.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (counter, first) = (counter.clone(), show.get());
        ui.scroll(state.clone(), Modifier::new().height(100.dp()), |ui| {
            for index in i32::from(!first)..6 {
                let counter = counter.clone();
                let tab = Modifier::new().tag(format!("tab {index}"));
                let tab = tab.on_click(move || counter.update(|count| *count += 1));
                ui.spacer(200.dp(), 40.dp(), tab);
            }
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = center(view.tagged_bounds("tab 0")?);
    let finger = |at| TouchSample {
        id: touch(1),
        at,
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(finger(start)));
    shown.set(false);
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    view.input(InputEvent::TouchMove(finger(
        start.offset(Dp::ZERO, -30.dp()),
    )));
    view.input(InputEvent::TouchUp(finger(
        start.offset(Dp::ZERO, -30.dp()),
    )));
    assert_eq!(scroll.offset(), 22.dp());
    assert_eq!(clicks.get(), 0);
    Ok(())
}
