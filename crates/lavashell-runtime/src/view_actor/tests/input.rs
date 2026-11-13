// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::observers::{HostKey, KeyboardFocus};
use crate::test_host::{FIELD_VIEWPORT, field_view, focus_field};
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::{Key, KeyStroke, Shortcut};
use lavashell_core::prelude::Modifier;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::InputEvent;
use lavashell_platform::{
    AccessibilityRequest, Button, ClipboardEvent, ClipboardRequest, DeviceEvent, FormatId,
    InputRequest, InputSerial, KeyAction, KeyboardDevice, PointerDevice, PressState,
    WindowControlRequest,
};
use std::cell::RefCell;

const POINTER: PointerDevice = PointerDevice(1);
const KEYBOARD: KeyboardDevice = KeyboardDevice(1);
const TYPED: u32 = 30;
const CONSUMED: u32 = 1;
const V: u32 = 47;
const PASTE: Shortcut = Shortcut::control(Key::Character('v'));

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

fn hold_control() -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Modifiers {
        keyboard: KEYBOARD,
        modifiers: InputModifiers {
            control: true,
            ..InputModifiers::default()
        },
    })
}

fn press_v() -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Key {
        keyboard: KEYBOARD,
        code: V,
        stroke: KeyStroke {
            key: Some(Key::Character('v')),
            text: Some("v".to_owned()),
        },
        action: KeyAction::Pressed,
    })
}

fn offer_text() -> PlatformEvent {
    let formats = vec![FormatId("text/plain".into())];
    PlatformEvent::Clipboard(ClipboardEvent::SelectionOffered(formats))
}

fn press_primary(at: Point) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::Button {
        pointer: POINTER,
        button: Button::Primary,
        state: PressState::Pressed,
        at,
        serial: InputSerial(1),
    })
}

fn pointer_moves(at: Point) -> PlatformEvent {
    PlatformEvent::Device(DeviceEvent::PointerMotion {
        pointer: POINTER,
        at,
    })
}

/// A window whose view shows a focused text field, which records in `received` each key that it receives.
fn field_window(
    modifier: Modifier,
    received: &Rc<RefCell<Vec<Shortcut>>>,
) -> Result<SurfaceSetup, TestError> {
    let recorded = Rc::clone(received);
    let record = move |shortcut: Shortcut| {
        recorded.borrow_mut().push(shortcut);
        true
    };
    let (mut view, _) = field_view(modifier.on_key(record))?;
    focus_field(&mut view);
    let size = FIELD_VIEWPORT.size;
    let window = Window::new("editor", "test.app").size(size.width, size.height);
    Ok(SurfaceSetup::Window(window, view))
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

fn reads_the_clipboard(request: &PlatformRequest) -> bool {
    matches!(
        request,
        PlatformRequest::Clipboard(ClipboardRequest::Receive(..))
    )
}

fn begins_move(request: &PlatformRequest) -> bool {
    matches!(
        request,
        PlatformRequest::WindowControl(WindowControlRequest::BeginMove {
            surface: WINDOW,
            pointer: POINTER,
            ..
        })
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
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    let requests = views.step([pointer_enters(WINDOW)])?;
    assert!(requests.iter().any(sets_cursor));
    Ok(())
}

#[test]
fn a_pointer_over_a_surface_that_is_not_open_gets_no_cursor() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
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
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
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
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
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
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    views.play([key(TYPED, "a", KeyAction::Pressed)])?;
    assert!(keys.borrow().is_empty());
    Ok(())
}

#[test]
fn a_paste_into_the_edited_field_reads_the_clipboard_instead_of_reaching_the_view() -> TestResult {
    let received = Rc::new(RefCell::new(Vec::new()));
    let field = field_window(Modifier::new(), &received)?;
    let mut views = views_of(vec![field], Capability::ALL)?;
    let paste = [offer_text(), focus(Some(WINDOW)), hold_control(), press_v()];
    let requests = views.step(paste)?;
    assert!(requests.iter().any(reads_the_clipboard));
    assert!(received.borrow().is_empty());
    Ok(())
}

#[test]
fn a_paste_without_an_edited_field_reaches_the_view() -> TestResult {
    let received = Rc::new(RefCell::new(Vec::new()));
    let read_only =
        Modifier::new().semantics_with(|semantics| semantics.state.form.read_only = true);
    let field = field_window(read_only, &received)?;
    let mut views = views_of(vec![field], Capability::ALL)?;
    let paste = [offer_text(), focus(Some(WINDOW)), hold_control(), press_v()];
    let requests = views.step(paste)?;
    assert!(!requests.iter().any(reads_the_clipboard));
    assert_eq!(*received.borrow(), [PASTE]);
    Ok(())
}

#[test]
fn a_press_that_begins_a_window_move_starts_no_drag_gesture() -> TestResult {
    let gestured = Rc::new(Cell::new(false));
    let seen = Rc::clone(&gestured);
    let header = Window::new("window", "test.app")
        .size(100.dp(), 100.dp())
        .drag_region(|_, _| true);
    let mut setup = runtime_setup(vec![SurfaceSetup::Window(header, empty_view())]);
    setup.drags.on_drag_source(move |_, _| {
        seen.set(true);
        None
    });
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    let origin = Point::new(10.dp(), 10.dp());
    let moved = Point::new(60.dp(), 60.dp());
    let drag = [
        pointer_enters(WINDOW),
        press_primary(origin),
        pointer_moves(moved),
    ];
    let requests = views.step(drag)?;
    assert!(requests.iter().any(begins_move));
    assert!(!gestured.get());
    Ok(())
}
