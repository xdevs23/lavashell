// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{HostKey, KeyboardFocus};
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::view::InputEvent;
use lavashell_platform::{
    AccessibilityRequest, DeviceEvent, InputRequest, KeyAction, KeyboardDevice, PointerDevice,
};
use std::cell::RefCell;

const POINTER: PointerDevice = PointerDevice(1);
const KEYBOARD: KeyboardDevice = KeyboardDevice(1);
const TYPED: u32 = 30;
const CONSUMED: u32 = 1;

fn pointer_enters(surface: SurfaceId) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::PointerEnter {
        pointer: POINTER,
        surface,
        at: Point::default(),
    })
}

fn focus(surface: Option<SurfaceId>) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::KeyboardFocus {
        keyboard: KEYBOARD,
        surface,
    })
}

/// A key of the keyboard that types `text`.
fn key(code: u32, text: &str, action: KeyAction) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Key {
        keyboard: KEYBOARD,
        code,
        stroke: KeyStroke {
            key: None,
            text: Some(text.to_owned()),
        },
        action,
    })
}

fn host_key(code: u32, pressed: bool, repeat: bool) -> HostKey {
    HostKey {
        surface: WINDOW,
        code,
        pressed,
        repeat,
    }
}

fn window_focus_change(focused: bool) -> KeyboardFocus {
    KeyboardFocus {
        surface: WINDOW,
        focused,
    }
}

fn sets_cursor(request: &PlatformRequest) -> bool {
    matches!(
        request,
        PlatformRequest::Input(InputRequest::SetCursor(POINTER, _))
    )
}

/// Whether `requests` tell the assistive technologies that the window has the keyboard focus, or has lost it.
fn sets_window_focus(requests: &[PlatformRequest], focused: bool) -> bool {
    let window_focus = AccessibilityRequest::SetWindowFocus {
        surface: WINDOW,
        focused,
    };
    (requests.iter()).any(|request| {
        matches!(request, PlatformRequest::Accessibility(request) if *request == window_focus)
    })
}

#[test]
fn a_pointer_that_enters_a_surface_gets_the_cursor_of_its_view() -> TestResult {
    let mut views = views_of(vec![window("window")])?;
    let requests = views.step([pointer_enters(WINDOW)])?;
    assert!(requests.iter().any(sets_cursor));
    Ok(())
}

#[test]
fn a_pointer_over_a_surface_that_is_not_open_gets_no_cursor() -> TestResult {
    let mut views = views_of(vec![window("window")])?;
    let requests = views.step([pointer_enters(SurfaceId(5))])?;
    assert!(!requests.iter().any(sets_cursor));
    Ok(())
}

#[test]
fn the_focus_observer_sees_each_surface_that_gains_or_loses_the_keyboard_focus() -> TestResult {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&changes);
    let mut setup = runtime_setup(vec![window("window")]);
    let observer = move |change: KeyboardFocus| recorded.borrow_mut().push(change);
    setup.observers.keyboard_focus = Some(Box::new(observer));
    let mut views = Views::start(setup, host([]))?;
    let gained = views.step([focus(Some(WINDOW))])?;
    let lost = views.step([focus(None)])?;
    let expected = [window_focus_change(true), window_focus_change(false)];
    assert_eq!(*changes.borrow(), expected);
    assert!(sets_window_focus(&gained, true));
    assert!(sets_window_focus(&lost, false));
    Ok(())
}

#[test]
fn a_consumed_key_reaches_the_key_observer_but_not_the_view() -> TestResult {
    let keys = Rc::new(RefCell::new(Vec::new()));
    let typed = Rc::new(RefCell::new(Vec::new()));
    let (recorded_keys, recorded_text) = (Rc::clone(&keys), Rc::clone(&typed));
    let mut setup = runtime_setup(vec![window("window")]);
    let observer = move |key: HostKey| {
        recorded_keys.borrow_mut().push(key);
        key.code == CONSUMED
    };
    let filter = move |_: SurfaceId, input: InputEvent| {
        if let InputEvent::Text(text) = &input {
            recorded_text.borrow_mut().push(text.clone());
        }
        Some(input)
    };
    setup.observers.key = Some(Box::new(observer));
    setup.input_filter = Some(Box::new(filter));
    let mut views = Views::start(setup, host([]))?;
    views.play([
        focus(Some(WINDOW)),
        key(TYPED, "a", KeyAction::Pressed),
        key(TYPED, "a", KeyAction::Repeated),
        key(TYPED, "a", KeyAction::Released),
        key(CONSUMED, "b", KeyAction::Pressed),
    ])?;
    let seen = [
        host_key(TYPED, true, false),
        host_key(TYPED, true, true),
        host_key(TYPED, false, false),
        host_key(CONSUMED, true, false),
    ];
    assert_eq!(*keys.borrow(), seen);
    assert_eq!(*typed.borrow(), ["a", "a"]);
    Ok(())
}

#[test]
fn a_key_without_keyboard_focus_is_not_shown_to_the_key_observer() -> TestResult {
    let keys = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&keys);
    let mut setup = runtime_setup(vec![window("window")]);
    let observer = move |key: HostKey| {
        recorded.borrow_mut().push(key);
        false
    };
    setup.observers.key = Some(Box::new(observer));
    let mut views = Views::start(setup, host([]))?;
    views.play([key(TYPED, "a", KeyAction::Pressed)])?;
    assert!(keys.borrow().is_empty());
    Ok(())
}
