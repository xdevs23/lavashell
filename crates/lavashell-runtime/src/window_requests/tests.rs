// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::Point;
use lavashell_core::units::DpExt;
use lavashell_platform::{InputSerial, PointerDevice};

const WINDOW: SurfaceId = SurfaceId::numbered(0);
const PRESS: PointerPress = PointerPress {
    pointer: PointerDevice(1),
    serial: InputSerial(4127),
};

#[test]
fn each_request_names_its_window_and_the_menu_opens_from_the_last_press() {
    let at = Point::new(212.dp(), 23.dp());
    let requests = [
        WindowRequest::SetMaximized(true),
        WindowRequest::Minimize,
        WindowRequest::SetFullscreen(false),
        WindowRequest::ShowMenu(at),
    ]
    .map(|request| to_window_control_request(WINDOW, request, Some(PRESS)));
    let expected = [
        WindowControlRequest::SetMaximized {
            surface: WINDOW,
            maximized: true,
        },
        WindowControlRequest::Minimize { surface: WINDOW },
        WindowControlRequest::SetFullscreen {
            surface: WINDOW,
            fullscreen: false,
        },
        WindowControlRequest::ShowMenu {
            surface: WINDOW,
            press: PRESS,
            at,
        },
    ]
    .map(Some);
    assert_eq!(requests, expected);
}

#[test]
fn the_menu_does_not_open_without_a_press_on_the_window() {
    let menu = WindowRequest::ShowMenu(Point::new(1.dp(), 1.dp()));
    assert_eq!(to_window_control_request(WINDOW, menu, None), None);
}
