// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::window::{Edges, WindowCapabilities};

fn configure(state: XdgState, capabilities: WindowManagerCapabilities) -> WindowConfigure {
    let mut configure = WindowConfigure::default();
    configure.state = state;
    configure.capabilities = capabilities;
    configure
}

#[test]
fn a_maximized_configure_reads_into_its_state_and_offered_controls() {
    let maximized = configure(
        XdgState::MAXIMIZED | XdgState::TILED_LEFT | XdgState::SUSPENDED,
        WindowManagerCapabilities::MINIMIZE | WindowManagerCapabilities::WINDOW_MENU,
    );
    let expected = WindowState {
        maximized: true,
        fullscreen: false,
        tiled: Edges::new([Edge::Left]),
        suspended: true,
        capabilities: WindowCapabilities::new([
            WindowControl::Minimize,
            WindowControl::WindowMenu,
            WindowControl::Move,
        ]),
    };
    assert_eq!(read_window_state(&maximized), expected);
}

#[test]
fn a_fullscreen_configure_reads_into_its_state_and_offered_controls() {
    let fullscreen = configure(
        XdgState::FULLSCREEN | XdgState::TILED_TOP | XdgState::TILED_RIGHT | XdgState::TILED_BOTTOM,
        WindowManagerCapabilities::MAXIMIZE | WindowManagerCapabilities::FULLSCREEN,
    );
    let expected = WindowState {
        maximized: false,
        fullscreen: true,
        tiled: Edges::new([Edge::Top, Edge::Right, Edge::Bottom]),
        suspended: false,
        capabilities: WindowCapabilities::new([
            WindowControl::Maximize,
            WindowControl::Fullscreen,
            WindowControl::Move,
        ]),
    };
    assert_eq!(read_window_state(&fullscreen), expected);
}

#[test]
fn a_compositor_that_sends_no_capabilities_offers_every_control() {
    let configure = WindowConfigure::default();
    assert_eq!(
        read_window_state(&configure).capabilities,
        WindowCapabilities::all()
    );
}
