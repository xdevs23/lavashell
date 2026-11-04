// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::{Point, Size};
use lavashell_core::keyboard::{Key, Shortcut};
use lavashell_core::units::{Density, DpExt};
use lavashell_core::view::{Cursor, View};
use lavashell_platform::SurfaceEvent;
use std::cell::Cell;
use std::rc::Rc;

const MOUSE: PointerDevice = PointerDevice(1);
const TABLET: PointerDevice = PointerDevice(2);
const KEYBOARD: KeyboardDevice = KeyboardDevice(3);
const WINDOW: SurfaceId = SurfaceId(0);
const PANEL: SurfaceId = SurfaceId(1);

fn route_all(
    router: &mut InputRouter,
    events: impl IntoIterator<Item = DeviceEvent>,
) -> Vec<RoutedInput> {
    (events.into_iter())
        .flat_map(|event| router.route(event))
        .collect()
}

fn at(x: i32, y: i32) -> Point {
    Point::new(x.dp(), y.dp())
}

fn by(pointer: PointerDevice, surface: SurfaceId, input: InputEvent) -> RoutedInput {
    RoutedInput::Input {
        surface,
        input,
        pointer: Some(pointer),
    }
}

fn enter(pointer: PointerDevice, surface: SurfaceId, at: Point) -> DeviceEvent {
    DeviceEvent::PointerEnter {
        pointer,
        surface,
        at,
    }
}

fn button(pointer: PointerDevice, button: Button, state: PressState, at: Point) -> DeviceEvent {
    DeviceEvent::Button {
        pointer,
        button,
        state,
        at,
    }
}

fn key(key: Key, state: PressState) -> DeviceEvent {
    let stroke = KeyStroke {
        key: Some(key),
        text: None,
    };
    DeviceEvent::Key {
        keyboard: KEYBOARD,
        stroke,
        state,
    }
}

fn focus(surface: Option<SurfaceId>) -> DeviceEvent {
    DeviceEvent::KeyboardFocus {
        keyboard: KEYBOARD,
        surface,
    }
}

#[test]
fn pointer_input_reaches_the_surface_under_the_pointer_at_the_position_of_each_event() {
    let mut router = InputRouter::new(None);
    let delta = at(0, 5);
    let received = route_all(
        &mut router,
        [
            enter(MOUSE, WINDOW, at(4, 2)),
            DeviceEvent::PointerMotion {
                pointer: MOUSE,
                at: at(6, 3),
            },
            button(MOUSE, Button::Primary, PressState::Pressed, at(7, 3)),
            button(MOUSE, Button::Primary, PressState::Released, at(8, 3)),
            button(MOUSE, Button::Secondary, PressState::Pressed, at(9, 3)),
            button(MOUSE, Button::Secondary, PressState::Released, at(9, 3)),
            DeviceEvent::Axis {
                pointer: MOUSE,
                at: at(9, 4),
                delta,
            },
            DeviceEvent::PointerLeave { pointer: MOUSE },
            button(MOUSE, Button::Primary, PressState::Pressed, at(9, 4)),
        ],
    );
    let expected = [
        InputEvent::Move(at(4, 2)),
        InputEvent::Move(at(6, 3)),
        InputEvent::Press(at(7, 3)),
        InputEvent::Release(at(8, 3)),
        InputEvent::Context(at(9, 3)),
        InputEvent::Scroll {
            at: at(9, 4),
            delta,
        },
        InputEvent::Leave,
    ];
    let expected: Vec<_> = (expected.into_iter())
        .map(|input| by(MOUSE, WINDOW, input))
        .collect();
    assert_eq!(received, expected);
}

#[test]
fn a_pointer_that_takes_over_a_surface_cancels_the_interaction_of_the_pointer_before() {
    let mut router = InputRouter::new(None);
    let received = route_all(
        &mut router,
        [
            enter(MOUSE, WINDOW, at(1, 1)),
            button(MOUSE, Button::Primary, PressState::Pressed, at(1, 1)),
            enter(TABLET, WINDOW, at(2, 2)),
            button(MOUSE, Button::Primary, PressState::Released, at(1, 1)),
            DeviceEvent::PointerLeave { pointer: MOUSE },
        ],
    );
    let expected = [
        by(MOUSE, WINDOW, InputEvent::Move(at(1, 1))),
        by(MOUSE, WINDOW, InputEvent::Press(at(1, 1))),
        RoutedInput::Cancel {
            surface: WINDOW,
            pointer: MOUSE,
        },
        by(TABLET, WINDOW, InputEvent::Move(at(2, 2))),
    ];
    assert_eq!(received, expected);
    assert_eq!(router.interacting_pointer(WINDOW), Some(TABLET));
}

#[test]
fn a_press_of_another_button_takes_the_surface_over_without_reaching_the_view() {
    let mut router = InputRouter::new(None);
    let received = route_all(
        &mut router,
        [
            enter(MOUSE, WINDOW, at(1, 1)),
            enter(TABLET, WINDOW, at(2, 2)),
            button(MOUSE, Button::Other, PressState::Pressed, at(1, 1)),
            button(MOUSE, Button::Other, PressState::Released, at(1, 1)),
        ],
    );
    let expected = [
        by(MOUSE, WINDOW, InputEvent::Move(at(1, 1))),
        RoutedInput::Cancel {
            surface: WINDOW,
            pointer: MOUSE,
        },
        by(TABLET, WINDOW, InputEvent::Move(at(2, 2))),
        RoutedInput::Cancel {
            surface: WINDOW,
            pointer: TABLET,
        },
    ];
    assert_eq!(received, expected);
    assert_eq!(router.interacting_pointer(WINDOW), Some(MOUSE));
}

#[test]
fn a_removed_pointer_cancels_its_interaction_with_every_surface() {
    let mut router = InputRouter::new(None);
    let received = route_all(
        &mut router,
        [
            enter(MOUSE, WINDOW, at(1, 1)),
            DeviceEvent::PointerLeave { pointer: MOUSE },
            enter(MOUSE, PANEL, at(1, 1)),
        ],
    );
    assert_eq!(received.len(), 3);
    let removed = router.route(DeviceEvent::PointerRemoved(MOUSE));
    let cancel = |surface| RoutedInput::Cancel {
        surface,
        pointer: MOUSE,
    };
    assert_eq!(removed, [cancel(WINDOW), cancel(PANEL)]);
    assert_eq!(router.interacting_pointer(WINDOW), None);
    let motion = DeviceEvent::PointerMotion {
        pointer: MOUSE,
        at: at(2, 2),
    };
    assert!(router.route(motion).is_empty());
}

#[test]
fn forgetting_a_surface_drops_the_pointers_on_it_so_a_later_pointer_enters_without_a_cancel() {
    let mut router = InputRouter::new(None);
    let entered = router.route(enter(MOUSE, WINDOW, at(1, 1)));
    assert_eq!(entered, [by(MOUSE, WINDOW, InputEvent::Move(at(1, 1)))]);
    router.forget_surface(WINDOW);
    let motion = DeviceEvent::PointerMotion {
        pointer: MOUSE,
        at: at(2, 2),
    };
    assert!(router.route(motion).is_empty());
    assert_eq!(router.interacting_pointer(WINDOW), None);
    let entered = router.route(enter(TABLET, WINDOW, at(3, 3)));
    assert_eq!(entered, [by(TABLET, WINDOW, InputEvent::Move(at(3, 3)))]);
}

#[test]
fn keys_reach_the_focused_surface_with_the_modifiers_held_on_their_keyboard() {
    let mut router = InputRouter::new(None);
    let control = InputModifiers {
        control: true,
        ..InputModifiers::default()
    };
    let received = route_all(
        &mut router,
        [
            key(Key::Tab, PressState::Pressed),
            focus(Some(WINDOW)),
            DeviceEvent::Modifiers {
                keyboard: KEYBOARD,
                modifiers: control,
            },
            key(Key::Character('a'), PressState::Pressed),
            key(Key::Enter, PressState::Released),
            focus(None),
            key(Key::Enter, PressState::Released),
        ],
    );
    let shortcut = Shortcut::control(Key::Character('a'));
    let expected = [
        input_without_pointer(WINDOW, InputEvent::Modifiers(control)),
        input_without_pointer(WINDOW, InputEvent::Shortcut(shortcut)),
        input_without_pointer(WINDOW, InputEvent::KeyRelease(Key::Enter)),
    ];
    assert_eq!(received, expected);
    assert_eq!(router.keyboard_focus(KEYBOARD), None);
    assert!(
        router
            .route(DeviceEvent::KeyboardRemoved(KEYBOARD))
            .is_empty()
    );
}

#[test]
fn the_filter_receives_each_pressed_key_and_passes_on_what_it_returns() {
    let filtered = Rc::new(Cell::new(0));
    let counted = filtered.clone();
    let filter: InputFilter = Box::new(move |_, input| {
        counted.set(counted.get() + 1);
        match input {
            InputEvent::Key {
                key: Key::Escape, ..
            } => None,
            other => Some(other),
        }
    });
    let mut router = InputRouter::new(Some(filter));
    let received = route_all(
        &mut router,
        [
            focus(Some(WINDOW)),
            key(Key::Escape, PressState::Pressed),
            key(Key::Tab, PressState::Pressed),
            key(Key::Space, PressState::Released),
        ],
    );
    let tab = InputEvent::Key {
        key: Key::Tab,
        shift: false,
    };
    let expected = [
        input_without_pointer(WINDOW, tab),
        input_without_pointer(WINDOW, InputEvent::KeyRelease(Key::Space)),
    ];
    assert_eq!(received, expected);
    assert_eq!(filtered.get(), 2);
}

#[test]
fn a_delivered_input_reaches_an_open_view_and_its_pointer_takes_the_cursor_of_the_view()
-> Result<(), crate::Error> {
    let mut records = SurfaceRecords::default();
    let view = View::new(lavashell_test_fixtures::facts(), |_| {});
    let surface = records.open(view, Size::new(10.dp(), 10.dp()), Density::ONE)?;
    let moved = by(MOUSE, surface, InputEvent::Move(at(1, 1)));
    let cursor = Some(InputRequest::SetCursor(MOUSE, Cursor::Default));
    assert_eq!(moved.deliver(&mut records), cursor);
    let cancel = RoutedInput::Cancel {
        surface,
        pointer: TABLET,
    };
    assert_eq!(cancel.deliver(&mut records), None);
    assert!(records.handle(surface, SurfaceEvent::Closed).is_some());
    let left = by(MOUSE, surface, InputEvent::Leave);
    assert_eq!(left.deliver(&mut records), None);
    Ok(())
}
