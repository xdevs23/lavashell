// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Capability, OutputId, PlatformEvent, PlatformRequest, StreamSetup, StreamTask, TimingEvent,
};
use lavashell_actor::{Clock, Outlet, Receiver};
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::{Attach, PresentationBackend, RendererStreams, SurfaceControl};
use std::collections::BTreeSet;
use std::sync::Arc;

/// A host system on which the runtime shows its surfaces. The platform runs on its own thread, where it translates
/// what the host does into events for the runtime and executes the requests of the runtime.
pub trait Platform: Sized + Send + 'static {
    type Error: std::error::Error + Send + 'static;
    /// Creates the GPU surfaces of the native surfaces for the renderer.
    type Presentation: PresentationBackend + Send + 'static;
    /// Makes the requests that each frame of a native surface applies, on the thread of the renderer.
    type Control: SurfaceControl + Send + 'static;
    /// The frames of the streams of the host, drawn by the renderer on its thread.
    type Streams: RendererStreams;

    /// # Errors
    ///
    /// Fails when the host cannot be reached.
    fn connect() -> Result<Connection<Self>, Self::Error>;

    /// The optional subsystems offered by the platform. The runtime reads them once, after the connection opens.
    fn capabilities(&self) -> BTreeSet<Capability>;

    /// Reports what the host does through `links` and executes each request from `links`, until every sender of the
    /// requests is gone. The native surfaces of the platform are destroyed when it returns.
    ///
    /// # Errors
    ///
    /// Fails when the connection to the host fails or a request cannot be executed.
    fn serve(self, links: PlatformLinks<Self::Control, Self::Streams>) -> Result<(), Self::Error>;
}

/// The open connection to the host. It contains the platform, the presentation, the streams for the renderer, the
/// report the host sent when the connection opened and the clock used by the run.
pub struct Connection<P: Platform> {
    pub platform: P,
    pub presentation: P::Presentation,
    pub streams: StreamSetup<P::Streams>,
    pub host: HostFacts,
    /// The clock on which the run measures every time and waits for each deadline.
    pub clock: Arc<dyn Clock>,
}

/// What a platform learns about the host when it connects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFacts {
    /// The outputs of the host when the connection opened.
    pub outputs: Vec<OutputId>,
    /// Whether the host reports when each frame became visible. Without these reports, the frame scheduler paces
    /// each surface by the refresh interval of the outputs that show it.
    pub reports_presentation: bool,
}

/// The links between a platform and the runtime actors. `S` is the streams type.
pub struct PlatformLinks<C: SurfaceControl, S> {
    pub reports: Reports<C, S>,
    /// What the runtime asks the platform to do. While the platform closes the intake, each request that does not
    /// replace an earlier one waits until the intake opens.
    pub requests: Receiver<PlatformRequest>,
}

/// The links through which a platform reports to the actors of the runtime. The platform never waits on the view, the
/// frame scheduler or the renderer.
pub struct Reports<C: SurfaceControl, S> {
    /// What the host did. An event that sets a whole value replaces the earlier event of that value while the view
    /// is busy.
    pub view: Outlet<PlatformEvent>,
    pub scheduler: Outlet<TimingEvent>,
    pub renderer: Outlet<SurfaceAttachment<C>>,
    /// Runs each task with the streams, after the attachments sent before.
    pub streams: Outlet<StreamTask<S>>,
    /// The clock used by the app, against which the platform measures each timing event.
    pub clock: Arc<dyn Clock>,
}

/// A report to an actor that has stopped is dropped. A stopped actor ends the run, and the end of the run stops the
/// requests and `Platform::serve`.
impl<C: SurfaceControl, S> Reports<C, S> {
    pub fn report(&self, event: impl Into<PlatformEvent>) {
        let _stopped_view_ends_the_run = self.view.push(event.into());
    }

    pub fn time(&self, event: TimingEvent) {
        let _stopped_scheduler_ends_the_run = self.scheduler.push(event);
    }

    pub fn attach(&self, attachment: SurfaceAttachment<C>) {
        let _stopped_renderer_ends_the_run = self.renderer.push(attachment);
    }

    pub fn run_on_streams(&self, task: StreamTask<S>) {
        let _stopped_renderer_ends_the_run = self.streams.push(task);
    }
}

/// What a platform sends to the renderer about a native surface.
pub enum SurfaceAttachment<C: SurfaceControl> {
    /// Creates the target of a new native surface.
    Attach(Box<Attach<C>>),
    /// Passes `message` to the control of `surface`, unless the surface was detached.
    Control {
        surface: SurfaceId,
        message: C::Message,
    },
    /// Drops the target and the control of the surface after the attachments sent before. The renderer then reports
    /// that it retired the surface.
    Detach(SurfaceId),
}
