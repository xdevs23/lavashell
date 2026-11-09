// SPDX-License-Identifier: GPL-3.0-only
/// A failure of the Wayland host. Each variant contains the error of one subsystem.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Connection(#[from] crate::ConnectionError),
    #[error(transparent)]
    Device(#[from] crate::DeviceError),
    #[error(transparent)]
    EventLoop(#[from] crate::EventLoopError),
    #[error(transparent)]
    FocusGrab(#[from] crate::FocusGrabError),
    #[error(transparent)]
    Layer(#[from] crate::LayerError),
    #[error(transparent)]
    Preferences(#[from] crate::PreferencesError),
    #[error(transparent)]
    Requests(#[from] crate::RequestsError),
    #[error(transparent)]
    SessionLock(#[from] crate::SessionLockError),
    #[error(transparent)]
    Watch(#[from] crate::WatchError),
    #[error(transparent)]
    Window(#[from] crate::WindowError),
}
