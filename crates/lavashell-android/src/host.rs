// SPDX-License-Identifier: GPL-3.0-only
mod error;
pub(crate) mod surface;

pub use error::AospError;

use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_actor::{Closed, lock_ignoring_poison};
use lavashell_platform::{PlatformLinks, PlatformRequest, Reports, StreamTask};
use lavashell_renderer::Painter;
use std::cell::RefCell;
use std::sync::{Arc, Mutex, mpsc};
use surface::HostSurface;

/// What reaches the platform of a run: the reports of the host, the arrival of requests and the tasks for the streams.
enum HostMessage {
    SurfaceCreated(NativeWindow),
    SurfaceChanged,
    /// The Surface is destroyed once `retired` is sent or dropped.
    SurfaceDestroyed(mpsc::SyncSender<()>),
    Frame,
    /// Requests of the runtime arrived, or every sender of the requests is gone.
    Requests,
    Streams(StreamTask<AospStreams>),
}

/// The link between an `AospHost` and the platform of the run started inside `AospHost::serve`.
pub(crate) struct HostLink {
    messages: mpsc::Receiver<HostMessage>,
    wake: mpsc::Sender<HostMessage>,
    remains: mpsc::Sender<Remains>,
}

/// What the platform leaves to `AospHost::serve` when it ends. A destroy callback returns once its sender drops, so
/// `serve` drops them only after the run has stopped the renderer.
struct Remains {
    _messages: mpsc::Receiver<HostMessage>,
    _destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostLink {
    /// Reports what the host does through `links` and executes each request from `links`, until every sender of the
    /// requests is gone.
    ///
    /// # Errors
    /// Fails for a request that an AOSP host does not support, for a second surface or Surface, and when the size of
    /// a native window cannot be read.
    pub(crate) fn serve(
        self,
        links: PlatformLinks<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let Self {
            messages,
            wake,
            remains,
        } = self;
        let PlatformLinks { reports, requests } = links;
        requests.on_arrival(move || {
            let _ended_platform_reads_nothing = wake.send(HostMessage::Requests);
        });
        let mut surface = HostSurface::default();
        let served = execute_messages(&messages, &requests, &reports, &mut surface);
        let left = Remains {
            _messages: messages,
            _destroying: surface.into_destroying(),
        };
        let _serve_receives_until_the_run_returns = remains.send(left);
        served
    }
}

/// Executes each message of the host until every sender of `requests` is gone.
fn execute_messages(
    messages: &mpsc::Receiver<HostMessage>,
    requests: &lavashell_actor::Receiver<PlatformRequest>,
    reports: &Reports<NativeWindow, AospStreams>,
    surface: &mut HostSurface,
) -> Result<(), AospError> {
    for message in messages {
        match message {
            // The last sender can be gone before the wake is set, so the requests are taken until none is left.
            HostMessage::Requests => loop {
                match requests.take_arrived() {
                    Ok(arrived) if arrived.is_empty() => break,
                    Ok(arrived) => {
                        for request in arrived {
                            surface.execute(request, reports)?;
                        }
                    }
                    Err(Closed) => return Ok(()),
                }
            },
            HostMessage::SurfaceCreated(window) => surface.created(window, reports)?,
            HostMessage::SurfaceChanged => surface.changed(reports)?,
            HostMessage::SurfaceDestroyed(retired) => surface.destroyed(retired, reports),
            HostMessage::Frame => surface.frame(reports),
            HostMessage::Streams(task) => reports.run_on_streams(task),
        }
    }
    Ok(())
}

thread_local! {
    static SERVED: RefCell<Option<HostLink>> = const { RefCell::new(None) };
}

/// Removes and returns the host link set by `serve` for this thread.
pub(crate) fn take_served_link() -> Option<HostLink> {
    SERVED.take()
}

/// Sets the host link of this thread until it drops, also when the run panics.
struct Served;

impl Served {
    fn set(link: Option<HostLink>) -> Self {
        SERVED.set(link);
        Self
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        drop(SERVED.take());
    }
}

struct Channel {
    sender: mpsc::Sender<HostMessage>,
    unserved: Mutex<Option<mpsc::Receiver<HostMessage>>>,
}

/// The callbacks called by the activity's JNI methods from any thread, for one run of the runtime. A callback that
/// needs the run blocks until `serve` has started it. Each clone sends to the same run.
#[derive(Clone)]
pub struct AospHost(Arc<Channel>);

/// The run that was started inside `AospHost::serve` has ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the run that was started inside `AospHost::serve` has ended")]
pub struct RunEnded;

impl AospHost {
    #[must_use]
    pub fn new() -> Self {
        let (sender, messages) = mpsc::channel();
        Self(Arc::new(Channel {
            sender,
            unserved: Mutex::new(Some(messages)),
        }))
    }

    /// Runs `run`, inside which `Aosp` connects to this host. Only the first run receives the host's reports. After
    /// that run ends, every callback returns immediately. A destroy callback that has not returned when the platform
    /// ends returns after `run` has returned, when the run has stopped the renderer.
    pub fn serve<T>(self, run: impl FnOnce() -> T) -> T {
        let (remains, _destroy_callbacks_until_run_returns) = mpsc::channel();
        let link = (lock_ignoring_poison(&self.0.unserved).take()).map(|messages| HostLink {
            messages,
            wake: self.0.sender.clone(),
            remains,
        });
        let _served = Served::set(link);
        run()
    }

    /// Reports the native window of the Surface created by the host. The renderer draws into it once the runtime opens
    /// its window.
    pub fn surface_created(&self, window: NativeWindow) {
        self.send(HostMessage::SurfaceCreated(window));
    }

    /// Reports that the host changed the size of the Surface. Call it in `surfaceChanged`.
    pub fn surface_changed(&self) {
        self.send(HostMessage::SurfaceChanged);
    }

    /// Reports that the host destroys the Surface, and returns once the renderer has retired it or the run has ended.
    /// Call it before the callback of the host for the destruction of the Surface returns.
    pub fn surface_destroyed(&self) {
        let (retired, waiter) = mpsc::sync_channel(1);
        self.send(HostMessage::SurfaceDestroyed(retired));
        let _retired_or_ended = waiter.recv();
    }

    /// Reports that the display is ready for the next frame. Call it in every frame callback of the Choreographer.
    pub fn frame(&self) {
        self.send(HostMessage::Frame);
    }

    /// Runs `task` with the streams and the painter on the renderer thread, after the frames drawn before, and
    /// returns its result.
    ///
    /// # Errors
    /// Fails when the run ended before the task ran.
    pub fn run_on_streams<T: Send + 'static>(
        &self,
        task: impl FnOnce(&mut AospStreams, &Painter) -> T + Send + 'static,
    ) -> Result<T, RunEnded> {
        let (reply, result) = mpsc::sync_channel(1);
        let task: StreamTask<AospStreams> = Box::new(move |streams, painter| {
            let _caller_left = reply.send(task(streams, painter));
        });
        self.send(HostMessage::Streams(task));
        result.recv().map_err(|mpsc::RecvError| RunEnded)
    }

    /// Sends `message` to the run. The message is dropped when the run has ended.
    fn send(&self, message: HostMessage) {
        let _ended_run_reads_nothing = self.0.sender.send(message);
    }
}

impl Default for AospHost {
    fn default() -> Self {
        Self::new()
    }
}
