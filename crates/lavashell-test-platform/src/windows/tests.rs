// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::window::Edges;
use lavashell_platform::{OutputId, WindowRole};

fn window(fullscreen: bool) -> Role {
    Role::Window(WindowRole {
        title: "app".to_owned(),
        app_id: "test.app".to_owned(),
        min_size: None,
        fullscreen,
        draws_decorations: false,
    })
}

#[test]
fn an_opened_window_is_fullscreen_as_requested_and_offers_every_control() {
    let opened = [true, false].map(|fullscreen| opened_window_state(&window(fullscreen)));
    let expected = [true, false].map(|fullscreen| {
        Some(WindowState {
            maximized: false,
            fullscreen,
            tiled: Edges::default(),
            suspended: false,
            capabilities: WindowCapabilities::all(),
        })
    });
    assert_eq!(opened, expected);
}

#[test]
fn a_surface_that_is_not_a_window_opens_without_a_window_state() {
    let lock = Role::Lock {
        output: OutputId(0),
    };
    assert_eq!(opened_window_state(&lock), None);
}
