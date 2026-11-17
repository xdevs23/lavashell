// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Capability, OutputId, PlatformEvent, PlatformRequest, StreamSetup, StreamTask, TimingEvent,
};
use lavashell_actor::{Clock, Outlet, Receiver};
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::{Attach, PresentationBackend, RendererStreams, SurfaceControl};
use std::collections::BTreeSet;
use std::sync::Arc;

pub trait Platform: Sized + Send + 'static {
    type Setup;
    type Error: std::error::Error + Send + 'static;
    type Presentation: PresentationBackend + Send + 'static;
    type Control: SurfaceControl + Send + 'static;
    type Streams: RendererStreams;

    fn connect(setup: Self::Setup) -> Result<Connection<Self>, Self::Error>;

    fn capabilities(&self) -> BTreeSet<Capability>;

    fn serve(self, links: PlatformLinks<Self::Control, Self::Streams>) -> Result<(), Self::Error>;
}

pub struct Connection<P: Platform> {
    pub platform: P,
    pub presentation: P::Presentation,
    pub streams: StreamSetup<P::Streams>,
    pub host: HostFacts,
    pub clock: Arc<dyn Clock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFacts {
    pub outputs: Vec<OutputId>,
    pub reports_presentation: bool,
}

pub struct PlatformLinks<C: SurfaceControl, S> {
    pub reports: Reports<C, S>,
    pub requests: Receiver<PlatformRequest>,
}

pub struct Reports<C: SurfaceControl, S> {
    pub view: Outlet<PlatformEvent>,
    pub scheduler: Outlet<TimingEvent>,
    pub renderer: Outlet<SurfaceAttachment<C>>,
    pub streams: Outlet<StreamTask<S>>,
    pub clock: Arc<dyn Clock>,
}

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

pub enum SurfaceAttachment<C: SurfaceControl> {
    Attach(Box<Attach<C>>),
    Control {
        surface: SurfaceId,
        message: C::Message,
    },
    Detach(SurfaceId),
}
