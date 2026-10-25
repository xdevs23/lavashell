// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::style::InteractionState;
use std::{cell::RefCell, rc::Rc};

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(200.0), Dp::new(100.0)), Density::ONE);

#[test]
fn tracked_interaction_follows_pointer_cancel_and_disabling_without_custom_styles() -> TestResult {
    let interaction = State::new(InteractionState::default());
    let enabled = State::new(true);
    let observed = interaction.clone();
    let available = enabled.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .on_pointer(|_| {})
                .enabled(available.get())
                .track_interaction(observed.clone()),
        );
    });
    let mut scene = Scene::default();
    let mut frame = |view: &mut View| advance_with(&mut scene, view, WINDOW, &mut Metrics);
    frame(&mut view)?;
    view.input(InputEvent::Move(Point::new(10.dp(), 10.dp())));
    frame(&mut view)?;
    assert!(interaction.get().pointer.hovered);
    view.input(InputEvent::Press(Point::new(10.dp(), 10.dp())));
    frame(&mut view)?;
    assert!(interaction.get().pointer.active);
    view.input(InputEvent::Move(Point::new(150.dp(), 60.dp())));
    frame(&mut view)?;
    assert!(!interaction.get().pointer.active);
    assert!(interaction.get().pointer.captured);
    view.input(InputEvent::CancelPointer);
    frame(&mut view)?;
    assert!(!interaction.get().pointer.active);
    assert!(!interaction.get().pointer.captured);
    enabled.set(false);
    frame(&mut view)?;
    assert!(interaction.get().disabled);
    assert!(!interaction.get().pointer.hovered);
    Ok(())
}

#[test]
fn pointer_phase_distinguishes_the_initial_press_from_stationary_and_captured_motion() -> TestResult
{
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let observed = observed.clone();
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new().on_pointer(move |event| {
                observed.borrow_mut().push(event.phase);
            }),
        );
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    let point = Point::new(10.dp(), 10.dp());
    view.input(InputEvent::Move(point));
    assert!(events.borrow().is_empty());
    view.input(InputEvent::Press(point));
    view.input(InputEvent::Move(point));
    view.input(InputEvent::Move(Point::new(150.dp(), 80.dp())));
    view.input(InputEvent::Release(point));
    view.input(InputEvent::Press(point));
    view.input(InputEvent::CancelPointer);
    view.input(InputEvent::Release(point));
    assert_eq!(
        *events.borrow(),
        [
            PointerPhase::Pressed,
            PointerPhase::Moved,
            PointerPhase::Moved,
            PointerPhase::Released,
            PointerPhase::Pressed,
            PointerPhase::Cancelled,
        ]
    );
    Ok(())
}
