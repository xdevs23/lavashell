// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_core::window::{Edge, WindowControl, WindowState};
use sctk::reexports::csd_frame::{WindowManagerCapabilities, WindowState as XdgState};
use sctk::shell::xdg::window::WindowConfigure;

pub(super) fn read_window_state(configure: &WindowConfigure) -> WindowState {
    WindowState {
        maximized: configure.is_maximized(),
        fullscreen: configure.is_fullscreen(),
        tiled: (Edge::ALL.into_iter())
            .filter(|&edge| is_tiled_at(configure, edge))
            .collect(),
        suspended: configure.state.contains(XdgState::SUSPENDED),
        capabilities: (WindowControl::ALL.into_iter())
            .filter(|&control| is_offered(configure, control))
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

fn is_offered(configure: &WindowConfigure, control: WindowControl) -> bool {
    let supports = |native| configure.capabilities.contains(native);
    match control {
        WindowControl::Maximize => supports(WindowManagerCapabilities::MAXIMIZE),
        WindowControl::Minimize => supports(WindowManagerCapabilities::MINIMIZE),
        WindowControl::Fullscreen => supports(WindowManagerCapabilities::FULLSCREEN),
        WindowControl::WindowMenu => supports(WindowManagerCapabilities::WINDOW_MENU),
        WindowControl::Move => true,
        WindowControl::Resize => !configure.is_maximized() && !configure.is_fullscreen(),
    }
}

#[cfg(test)]
mod tests;
