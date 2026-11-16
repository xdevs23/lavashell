// SPDX-License-Identifier: GPL-3.0-only
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
    Preferences(#[from] crate::PreferencesError),
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
