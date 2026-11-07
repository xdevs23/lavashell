// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Closed, JoinError, SpawnError};
use lavashell_core::geometry::{Rect, Size};
use lavashell_core::shape::PointsError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{FiniteDp, NonNegativeDp, NumberError};
use lavashell_core::view::FrameError;
use lavashell_renderer::{InputSizeError, OutlineError};
use lavashell_runtime::{RendererError, RendererStopped};
use sctk::data_device_manager::data_offer::DataOfferError;
use sctk::reexports::calloop;
use sctk::reexports::calloop::futures::ExecutorDestroyed;
use sctk::reexports::client::ConnectError;
use sctk::reexports::client::backend::WaylandError;
use sctk::reexports::client::globals::{BindError, GlobalError};
use smithay_client_toolkit as sctk;
use std::collections::TryReserveError;
use std::io;
use std::num::TryFromIntError;
use std::string::FromUtf8Error;

/// A failure of the Wayland host, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the app has a session lock already")]
    SessionLockConfigured,
    #[error("the app has a per-output layer factory already")]
    OutputLayersConfigured,
    #[error("the session-lock handle belongs to another lock")]
    SessionLockReused,
    #[error("the window size {0:?} is not positive and finite or does not fit the protocol")]
    WindowSize(Size),
    #[error("the minimum window size {0:?} is not finite or does not fit the protocol")]
    MinimumWindowSize(Size),
    #[error("the app did not request any surface")]
    NoSurfaces,
    #[error(transparent)]
    Runtime(#[from] lavashell_runtime::Error),
    #[error("the connection to the compositor could not be opened")]
    Connect(#[source] ConnectError),
    #[error("the globals of the compositor could not be read")]
    ReadGlobals(#[source] GlobalError),
    #[error("the event loop could not be created")]
    CreateEventLoop(#[source] calloop::Error),
    #[error("the Wayland connection could not join the event loop")]
    WatchConnection(#[source] calloop::Error),
    #[error("the accessibility requests could not join the event loop")]
    WatchAccessibility(#[source] calloop::Error),
    #[error("the reports of the scene builder could not join the event loop")]
    WatchScene(#[source] calloop::Error),
    #[error("the reports of the renderer could not join the event loop")]
    WatchRenderer(#[source] calloop::Error),
    #[error("the system preferences could not join the event loop")]
    WatchPreferences(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be created")]
    CreatePreferenceWatch(#[source] calloop::Error),
    #[error("the watch of the system preferences could not be started")]
    SchedulePreferenceWatch(#[source] ExecutorDestroyed),
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] calloop::Error),
    #[error("a transfer pipe could not join the event loop")]
    WatchPipe(#[source] calloop::Error),
    #[error("the event loop failed to dispatch")]
    DispatchEvents(#[source] calloop::Error),
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
    #[error("the renderer could not start")]
    StartRenderer(#[source] SpawnError<RendererError<ControlError>>),
    #[error("the renderer failed")]
    RunRenderer(#[source] JoinError<RendererError<ControlError>>),
    #[error("the renderer stopped")]
    RendererStopped(#[from] RendererStopped),
    #[error("the shaper that measures text for the views could not be created")]
    CreateShaper(#[source] lavashell_renderer::Error),
    #[error("a session lock needs an advertised output")]
    SessionLockWithoutOutput,
    #[error("a lock surface was requested without a session lock")]
    LockSurfaceWithoutLock,
    #[error("the compositor refused or ended the session lock")]
    SessionLockEnded,
    #[error("the layout of a new layer does not fit the protocol")]
    LayerLayout(#[source] LayerLayoutError),
    #[error("a contour of a window shape is not a valid polygon")]
    ShapeContour(#[from] PointsError),
    #[error("the path of a window shape could not be built")]
    ShapePath(#[source] OutlineError),
    #[error("the input region of a window shape could not be computed")]
    ShapeInput(#[source] InputSizeError),
    #[error("a surface could not show a frame of its view")]
    ShowScene(#[source] FrameError<Closed>),
    #[error("the scene builder stopped")]
    SceneStopped,
    #[error("surface {0:?} is not open")]
    SurfaceNotOpen(SurfaceId),
    #[error("the runtime made panel requests for the surface {0:?}, which has no layer role")]
    PanelWithoutLayer(SurfaceId),
    #[error("the compositor offers no clipboard")]
    ClipboardUnavailable,
    #[error("the clipboard needs the serial of an input event")]
    NoInputSerial,
    #[error("the surface has no focused input device")]
    NoFocusedInputDevice,
    #[error("the surface has more than one focused input device")]
    SeveralFocusedInputDevices,
    #[error("the clipboard is empty")]
    ClipboardEmpty,
    #[error("the clipboard has none of the requested formats")]
    NoRequestedFormat,
    #[error("the dragged data could not be received")]
    ReceiveDrag(#[source] io::Error),
    #[error("the clipboard data could not be received")]
    ReceiveSelection(#[source] DataOfferError),
    #[error("the transfer pipe could not be made non-blocking")]
    ConfigurePipe(#[source] rustix::io::Errno),
    #[error("the capacity of the transfer pipe could not be read")]
    PipeCapacity(#[source] rustix::io::Errno),
    #[error("the transfer pipe could not be read")]
    ReadPipe(#[source] io::Error),
    #[error("the transfer pipe could not be written")]
    WritePipe(#[source] io::Error),
    #[error("the memory for the transfer could not be reserved")]
    ReserveTransfer(#[source] TryReserveError),
    #[error("the transferred text is not UTF-8")]
    DecodeText(#[source] FromUtf8Error),
    #[error("the compositor offers no drag and drop")]
    DragUnavailable,
    #[error("the compositor cannot report the end of a drag")]
    DragCompletionUnavailable,
    #[error("a drag needs a supported action")]
    DragWithoutAction,
    #[error("the destination asked for a format that the drag did not offer")]
    UnadvertisedFormat,
    #[error("the drag finished without a transfer and a supported action")]
    DragWithoutTransfer,
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}

/// Why a transfer ended before it finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Cancellation {
    #[error("the app closed")]
    AppClosed,
    #[error("the input device of the transfer disappeared")]
    InputDeviceGone,
    #[error("the surface of the transfer closed")]
    SurfaceClosed,
    #[error("the compositor cancelled the data source")]
    SourceCancelled,
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
