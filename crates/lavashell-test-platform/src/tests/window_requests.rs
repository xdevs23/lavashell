// SPDX-License-Identifier: GPL-3.0-only
use super::{APP_ID, KEYBOARD, SIXTY_HERTZ, TestResult, app, key_a};
use crate::{HostScript, TestPlatform};
use lavashell_core::geometry::Point;
use lavashell_core::prelude::{ContextRequest, Density, DpExt, Modifier};
use lavashell_core::surface::SurfaceId;
use lavashell_core::window::{ResizeEdge, WINDOW, WindowHandle, WindowRegion, WindowRequest};
use lavashell_platform::{
    Button, DeviceEvent, InputSerial, PointerDevice, PointerPress, PressState, WindowControlRequest,
};
use lavashell_runtime::{App, SetupError, run};
use std::cell::RefCell;
use std::rc::Rc;

const SURFACE: SurfaceId = SurfaceId::numbered(0);
const PRESS: PointerPress = PointerPress {
    pointer: PointerDevice(1),
    serial: InputSerial(4127),
};

type ComposedWindow = Rc<RefCell<Option<WindowHandle>>>;

fn app_requesting_on_each_key(
    requests: impl Fn(&WindowHandle) -> Vec<WindowRequest> + 'static,
) -> Result<(App, ComposedWindow), SetupError> {
    let window = Rc::new(RefCell::new(None));
    let composed = Rc::clone(&window);
    let requesting = Rc::clone(&window);
    let app = app(move |ui| *composed.borrow_mut() = ui.local(&WINDOW))?.on_keyboard(move |_| {
        if let Some(window) = &*requesting.borrow() {
            for request in requests(window) {
                assert_eq!(window.request(request), Ok(()));
            }
        }
        false
    });
    Ok((app, window))
}

fn pressing(button: Button, at: Point) -> HostScript {
    HostScript::new(Density::ONE)
        .report(DeviceEvent::PointerEnter {
            pointer: PRESS.pointer,
            surface: SURFACE,
            at,
        })
        .report(DeviceEvent::Button {
            pointer: PRESS.pointer,
            button,
            state: PressState::Pressed,
            at,
            serial: PRESS.serial,
        })
}

fn typing_one_key() -> HostScript {
    (HostScript::new(Density::ONE).focus_keyboard(KEYBOARD, APP_ID)).report(key_a())
}

#[test]
fn a_maximize_comes_back_as_a_configure_and_a_minimize_is_recorded() -> TestResult {
    let (app, window) = app_requesting_on_each_key(|_| {
        vec![WindowRequest::SetMaximized(true), WindowRequest::Minimize]
    })?;
    let script = typing_one_key().record_until_rest(SIXTY_HERTZ);
    let (ran, frames) = script.serve(|host| run::<TestPlatform>(app, host));
    ran?;
    let maximized = (window.borrow().as_ref()).map(|window| window.state().maximized);
    assert_eq!(maximized, Some(true));
    assert_eq!(
        frames?.window_requests(),
        [WindowControlRequest::Minimize { surface: SURFACE }]
    );
    Ok(())
}

#[test]
fn each_fullscreen_request_flips_the_state_of_the_window() -> TestResult {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seeing = Rc::clone(&seen);
    let (app, window) = app_requesting_on_each_key(move |window| {
        let fullscreen = window.state().fullscreen;
        seeing.borrow_mut().push(fullscreen);
        vec![WindowRequest::SetFullscreen(!fullscreen)]
    })?;
    let script = (typing_one_key().record_until_rest(SIXTY_HERTZ))
        .report(key_a())
        .record_until_rest(SIXTY_HERTZ);
    let (ran, _frames) = script.serve(|host| run::<TestPlatform>(app, host));
    ran?;
    assert_eq!(*seen.borrow(), [false, true]);
    let fullscreen = (window.borrow().as_ref()).map(|window| window.state().fullscreen);
    assert_eq!(fullscreen, Some(false));
    Ok(())
}

#[test]
fn a_right_click_opens_the_window_menu_with_the_serial_of_its_press() -> TestResult {
    let at = Point::new(21.dp(), 23.dp());
    let app = app(|ui| {
        let window = ui.local(&WINDOW);
        let open_menu = move |request: ContextRequest| {
            if let Some(window) = &window {
                let menu = WindowRequest::ShowMenu(request.position);
                assert_eq!(window.request(menu), Ok(()));
            }
        };
        ui.spacer(100.dp(), 100.dp(), Modifier::new().on_context(open_menu));
    })?;
    let script = pressing(Button::Secondary, at);
    let (ran, frames) = script.serve(|host| run::<TestPlatform>(app, host));
    ran?;
    let menu = WindowControlRequest::ShowMenu {
        surface: SURFACE,
        press: PRESS,
        at,
    };
    assert_eq!(frames?.window_requests(), [menu]);
    Ok(())
}

#[test]
fn the_test_platform_records_the_move_started_by_a_press_in_a_drag_region() -> TestResult {
    let app = app(|ui| {
        let whole_window = Modifier::new().window_region(WindowRegion::Drag);
        ui.spacer(100.dp(), 100.dp(), whole_window);
    })?;
    let script = pressing(Button::Primary, Point::new(21.dp(), 23.dp()));
    let (ran, frames) = script.serve(|host| run::<TestPlatform>(app, host));
    ran?;
    let begin_move = WindowControlRequest::BeginMove {
        surface: SURFACE,
        press: PRESS,
    };
    assert_eq!(frames?.window_requests(), [begin_move]);
    Ok(())
}

#[test]
fn the_test_platform_records_the_resize_started_by_a_press_in_a_resize_region() -> TestResult {
    let edge = ResizeEdge::BottomRight;
    let app = app(move |ui| {
        let whole_window = Modifier::new().window_region(WindowRegion::Resize(edge));
        ui.spacer(100.dp(), 100.dp(), whole_window);
    })?;
    let script = pressing(Button::Primary, Point::new(21.dp(), 23.dp()));
    let (ran, frames) = script.serve(|host| run::<TestPlatform>(app, host));
    ran?;
    let begin_resize = WindowControlRequest::BeginResize {
        surface: SURFACE,
        press: PRESS,
        edge,
    };
    assert_eq!(frames?.window_requests(), [begin_resize]);
    Ok(())
}

#[test]
fn a_window_menu_requested_from_a_key_does_not_open() -> TestResult {
    let (app, _window) =
        app_requesting_on_each_key(|_| vec![WindowRequest::ShowMenu(Point::new(1.dp(), 1.dp()))])?;
    let (ran, frames) = typing_one_key().serve(|host| run::<TestPlatform>(app, host));
    ran?;
    assert_eq!(frames?.window_requests(), []);
    Ok(())
}
