// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk::reexports::client::backend::WaylandError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Connection(#[from] crate::ConnectionError),
    #[error(transparent)]
    Device(#[from] crate::DeviceError),
    #[error(transparent)]
    DragSource(#[from] crate::DragSourceError),
    #[error(transparent)]
    EventLoop(#[from] crate::EventLoopError),
    #[error(transparent)]
    FocusGrab(#[from] crate::FocusGrabError),
    #[error(transparent)]
    Layer(#[from] crate::LayerError),
    #[error(transparent)]
    Requests(#[from] crate::RequestsError),
    #[error(transparent)]
    SessionLock(#[from] crate::SessionLockError),
    #[error(transparent)]
    Watch(#[from] crate::WatchError),
    #[error(transparent)]
    Window(#[from] crate::WindowError),
    #[error(transparent)]
    WindowControl(#[from] crate::WindowControlError),
}

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Connection(error) => error.next_step(),
            Self::Device(error) => error.next_step(),
            Self::DragSource(error) => error.next_step(),
            Self::EventLoop(error) => error.next_step(),
            Self::FocusGrab(error) => error.next_step(),
            Self::Layer(error) => error.next_step(),
            Self::Requests(error) => error.next_step(),
            Self::SessionLock(error) => error.next_step(),
            Self::Watch(error) => error.next_step(),
            Self::Window(error) => error.next_step(),
            Self::WindowControl(error) => error.next_step(),
        }
    }
}

pub(crate) fn next_step_after(error: &WaylandError) -> NextStep {
    match error {
        WaylandError::Io(_) => NextStep::StartInDesktopSession,
        WaylandError::Protocol(_) => NextStep::ReportTheFailure,
    }
}

#[cfg(test)]
mod tests;
