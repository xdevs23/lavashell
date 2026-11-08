// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{FiniteDp, NonNegativeDp, NumberError};
use sctk::reexports::calloop;
use sctk::reexports::calloop::futures::ExecutorDestroyed;
use sctk::reexports::client::ConnectError;
use sctk::reexports::client::backend::WaylandError;
use sctk::reexports::client::globals::{BindError, GlobalError};
use smithay_client_toolkit as sctk;
use std::io;
use std::num::TryFromIntError;

/// A setup of an app that cannot run on Wayland, named after the part of the setup that is wrong.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("the app has a session lock already")]
    SessionLockConfigured,
    #[error("the app has a per-output layer factory already")]
    OutputLayersConfigured,
    #[error("the window size {0:?} is not positive and finite or does not fit the protocol")]
    WindowSize(Size),
    #[error("the minimum window size {0:?} is not finite or does not fit the protocol")]
    MinimumWindowSize(Size),
    #[error("the app did not request any surface")]
    NoSurfaces,
}

/// A failure of the Wayland host, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the runtime requested a window whose minimum size {0:?} does not fit the protocol")]
    MinimumWindowSize(Size),
    #[error("the connection to the compositor could not be opened")]
    Connect(#[source] ConnectError),
    #[error("the globals of the compositor could not be read")]
    ReadGlobals(#[source] GlobalError),
    #[error("the event loop could not be created")]
    CreateEventLoop(#[source] calloop::Error),
    #[error("the Wayland connection could not join the event loop")]
    WatchConnection(#[source] calloop::Error),
    #[error("the system preferences could not join the event loop")]
    WatchPreferences(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be created")]
    CreatePreferenceWatch(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be started")]
    SchedulePreferenceWatch(#[source] ExecutorDestroyed),
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] calloop::Error),
    #[error("a watched file descriptor could not be watched again")]
    ResumeWatch(#[source] calloop::Error),
    #[error("the event loop failed to dispatch")]
    DispatchEvents(#[source] calloop::Error),
    #[error("the wake for the requests of the runtime could not be created")]
    CreateRequestWake(#[source] io::Error),
    #[error("the requests of the runtime could not join the event loop")]
    WatchRequests(#[source] calloop::Error),
    #[error("the requests of the runtime could not be paused")]
    PauseRequests(#[source] calloop::Error),
    #[error("the requests of the runtime could not be resumed")]
    ResumeRequests(#[source] calloop::Error),
    #[error("the socket of the connection could not be duplicated")]
    DuplicateSocket(#[source] io::Error),
    #[error("the writability of the connection could not join the event loop")]
    WatchWritable(#[source] calloop::Error),
    #[error("the requests could not be sent to the compositor")]
    Flush(#[source] WaylandError),
    #[error("wl_shm could not be bound")]
    BindShm(#[source] BindError),
    #[error("wl_compositor could not be bound")]
    BindCompositor(#[source] BindError),
    #[error("ext-session-lock-v1 could not be bound")]
    BindSessionLock(#[source] BindError),
    #[error("the compositor offers no xdg-shell")]
    NoXdgShell,
    #[error("the compositor offers no layer-shell")]
    NoLayerShell,
    #[error("the runtime requested the lock surface {0:?} while the session is not locked")]
    LockSurfaceWithoutLock(SurfaceId),
    #[error("the runtime requested an unlock while the session is not locked")]
    UnlockWithoutLock,
    #[error("the layout of a new layer does not fit the protocol")]
    LayerLayout(#[source] LayerLayoutError),
    #[error("the runtime made panel requests for the surface {0:?}, which has no layer role")]
    PanelWithoutLayer(SurfaceId),
    #[error(
        "the runtime made a focus grab request for the surface {0:?}, which has no native surface"
    )]
    FocusGrabWithoutSurface(SurfaceId),
    #[error("the serial of an input event does not fit a Wayland serial")]
    ProtocolSerial(#[source] TryFromIntError),
}

/// A failure of a run of an app on Wayland.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("the app is not set up to run")]
    Setup(#[source] SetupError),
    #[error("the run of the app failed")]
    Run(#[source] lavashell_runtime::RunError<Error, ControlError>),
}

/// A failure of a request for the size or the state of a native surface, sent with a frame or with a commit without a
/// frame.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("no buffer of whole pixels covers one logical pixel")]
    BufferSize(#[source] NumberError),
    #[error("the buffer scale of a surface does not fit the protocol")]
    BufferScale(#[source] TryFromIntError),
    #[error("the size {0:?} of a surface does not fit the viewport destination")]
    Destination(Size),
    #[error("the rectangle {0:?} of a region does not fit the protocol")]
    Region(Rect),
    #[error("the layout of a layer does not fit the protocol")]
    Layout(#[from] LayerLayoutError),
    #[error("the requests of a surface could not be sent to the compositor")]
    Flush(#[source] WaylandError),
}

/// A layout of a layer whose lengths do not fit the layer-shell protocol.
#[derive(Debug, thiserror::Error)]
pub enum LayerLayoutError {
    #[error("the layer length {0:?} does not fit the protocol")]
    Length(NonNegativeDp),
    #[error("the exclusive zone {0:?} does not fit the protocol")]
    ExclusiveZone(NonNegativeDp),
    #[error("the extent of a layer is too long to reserve")]
    ReserveExtent(#[source] TryFromIntError),
    #[error("the layer margin {0:?} does not fit the protocol")]
    Margin(FiniteDp),
}
