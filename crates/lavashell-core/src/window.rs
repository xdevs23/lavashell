// SPDX-License-Identifier: GPL-3.0-only
use crate::flag_set::flag_set;
use crate::geometry::Point;
use crate::local::Local;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowRequest {
    SetMaximized(bool),
    Minimize,
    SetFullscreen(bool),
    ShowMenu(Point),
}

impl WindowRequest {
    #[must_use]
    pub const fn required_control(self) -> WindowControl {
        match self {
            Self::SetMaximized(_) => WindowControl::Maximize,
            Self::Minimize => WindowControl::Minimize,
            Self::SetFullscreen(_) => WindowControl::Fullscreen,
            Self::ShowMenu(_) => WindowControl::WindowMenu,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WindowRequestRefusal {
    #[error("the compositor does not offer {0:?} for this window")]
    NotOffered(WindowControl),
    #[error("the window is closed")]
    WindowClosed,
}

#[derive(Debug, Default)]
pub struct WindowRequestQueue(Rc<RefCell<Vec<WindowRequest>>>);

impl WindowRequestQueue {
    #[must_use]
    pub fn handle(&self, state: WindowState) -> WindowHandle {
        WindowHandle {
            state,
            requests: Rc::downgrade(&self.0),
        }
    }

    #[must_use]
    pub fn take(&self) -> Vec<WindowRequest> {
        self.0.take()
    }
}

#[derive(Clone, Debug)]
pub struct WindowHandle {
    state: WindowState,
    requests: Weak<RefCell<Vec<WindowRequest>>>,
}

impl WindowHandle {
    #[must_use]
    pub const fn state(&self) -> WindowState {
        self.state
    }

    pub fn request(&self, request: WindowRequest) -> Result<(), WindowRequestRefusal> {
        let control = request.required_control();
        if !self.state.capabilities.contains(control) {
            return Err(WindowRequestRefusal::NotOffered(control));
        }
        let requests = (self.requests.upgrade()).ok_or(WindowRequestRefusal::WindowClosed)?;
        requests.borrow_mut().push(request);
        Ok(())
    }
}

impl PartialEq for WindowHandle {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
    }
}

pub static WINDOW: Local<Option<WindowHandle>> = Local::new(|| None);

#[cfg(test)]
mod tests;
