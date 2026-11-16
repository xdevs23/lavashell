// SPDX-License-Identifier: GPL-3.0-only
use crate::flag_set::flag_set;
use crate::local::Local;

flag_set!(
    pub enum Edge in Edges {
        Top => top,
        Right => right,
        Bottom => bottom,
        Left => left,
    }
);

flag_set!(
    pub enum WindowControl in WindowCapabilities {
        Maximize => maximize,
        Minimize => minimize,
        Fullscreen => fullscreen,
        WindowMenu => window_menu,
    }
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowState {
    pub maximized: bool,
    pub fullscreen: bool,
    pub tiled: Edges,
    pub suspended: bool,
    pub capabilities: WindowCapabilities,
}

impl WindowState {
    #[must_use]
    pub fn floating(self) -> bool {
        !self.maximized && !self.fullscreen && self.tiled.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowHandle {
    state: WindowState,
}

impl WindowHandle {
    #[must_use]
    pub const fn new(state: WindowState) -> Self {
        Self { state }
    }

    #[must_use]
    pub const fn state(self) -> WindowState {
        self.state
    }
}

pub static WINDOW: Local<Option<WindowHandle>> = Local::new(|| None);

#[cfg(test)]
mod tests;
