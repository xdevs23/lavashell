// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_core::window::{Edge, WindowControl, WindowState};
use sctk::reexports::csd_frame::{WindowManagerCapabilities, WindowState as XdgState};
use sctk::shell::xdg::window::WindowConfigure;

pub(super) fn read_window_state(configure: &WindowConfigure) -> WindowState {
    let offered = |control| configure.capabilities.contains(native_capability(control));
    WindowState {
        maximized: configure.is_maximized(),
        fullscreen: configure.is_fullscreen(),
        tiled: (Edge::ALL.into_iter())
            .filter(|&edge| is_tiled_at(configure, edge))
            .collect(),
        suspended: configure.state.contains(XdgState::SUSPENDED),
        capabilities: (WindowControl::ALL.into_iter())
            .filter(|&control| offered(control))
            .collect(),
    }
}

fn is_tiled_at(configure: &WindowConfigure, edge: Edge) -> bool {
    match edge {
        Edge::Top => configure.is_tiled_top(),
        Edge::Right => configure.is_tiled_right(),
        Edge::Bottom => configure.is_tiled_bottom(),
        Edge::Left => configure.is_tiled_left(),
    }
}

fn native_capability(control: WindowControl) -> WindowManagerCapabilities {
    match control {
        WindowControl::Maximize => WindowManagerCapabilities::MAXIMIZE,
        WindowControl::Minimize => WindowManagerCapabilities::MINIMIZE,
        WindowControl::Fullscreen => WindowManagerCapabilities::FULLSCREEN,
        WindowControl::WindowMenu => WindowManagerCapabilities::WINDOW_MENU,
    }
}

#[cfg(test)]
mod tests;
