// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::Dp;

#[test]
fn a_set_offers_by_name_only_the_flags_it_was_built_from() {
    let tiled = Edges::new([Edge::Left, Edge::Top]);
    let sides = [tiled.top(), tiled.right(), tiled.bottom(), tiled.left()];
    assert_eq!(sides, [true, false, false, true]);
    let offered = WindowCapabilities::new([WindowControl::WindowMenu]);
    let controls = [
        offered.maximize(),
        offered.minimize(),
        offered.fullscreen(),
        offered.window_menu(),
        offered.move_(),
    ];
    assert_eq!(controls, [false, false, false, true, false]);
    assert!(Edges::all().bottom());
    assert!(Edges::default().is_empty());
}

#[test]
fn a_set_prints_the_names_of_its_flags() {
    let offered = WindowCapabilities::new([WindowControl::Fullscreen, WindowControl::Maximize]);
    assert_eq!(format!("{offered:?}"), "{Maximize, Fullscreen}");
    assert_eq!(format!("{:?}", Edges::default()), "{}");
}

#[test]
fn a_handle_sends_only_requests_for_the_controls_the_compositor_offers() {
    let queue = WindowRequestQueue::default();
    let state = WindowState {
        capabilities: WindowCapabilities::new([WindowControl::Maximize, WindowControl::WindowMenu]),
        ..WindowState::default()
    };
    let window = queue.handle(state);
    let menu = WindowRequest::ShowMenu(Point::new(Dp::new(212.0), Dp::new(23.0)));
    let answers = [
        WindowRequest::SetMaximized(true),
        WindowRequest::Minimize,
        WindowRequest::SetFullscreen(true),
        menu,
    ]
    .map(|request| window.request(request));
    let not_offered = |control| Err(WindowRequestRefusal::NotOffered(control));
    assert_eq!(
        answers,
        [
            Ok(()),
            not_offered(WindowControl::Minimize),
            not_offered(WindowControl::Fullscreen),
            Ok(()),
        ]
    );
    assert_eq!(queue.take(), [WindowRequest::SetMaximized(true), menu]);
}

#[test]
fn a_handle_refuses_requests_once_its_window_closed() {
    let queue = WindowRequestQueue::default();
    let window = queue.handle(WindowState {
        capabilities: WindowCapabilities::all(),
        ..WindowState::default()
    });
    drop(queue);
    assert_eq!(
        window.request(WindowRequest::Minimize),
        Err(WindowRequestRefusal::WindowClosed)
    );
}

#[test]
fn a_window_floats_only_while_neither_maximized_nor_fullscreen_nor_tiled() {
    let floating = WindowState::default();
    let maximized = WindowState {
        maximized: true,
        ..floating
    };
    let fullscreen = WindowState {
        fullscreen: true,
        ..floating
    };
    let tiled = WindowState {
        tiled: Edges::new([Edge::Right]),
        ..floating
    };
    let suspended = WindowState {
        suspended: true,
        capabilities: WindowCapabilities::all(),
        ..floating
    };
    let floats = [floating, maximized, fullscreen, tiled, suspended].map(WindowState::floating);
    assert_eq!(floats, [true, false, false, false, true]);
}
