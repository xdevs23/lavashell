// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Attach, Backend, Capability, OutputId, PlatformEvent, PlatformRequest, StreamTask,
    SurfaceControl, TimingEvent,
};
use lavashell_actor::{Clock, Outlet, Receiver};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeSet;
use std::sync::Arc;

pub trait Platform: Sized + Send + 'static {
    type Setup;
    type Error: std::error::Error + Send + 'static;
    type Backend: Backend;
    type Control: SurfaceControl + Send + 'static;

    fn connect(setup: Self::Setup) -> Result<Connection<Self>, Self::Error>;

    fn capabilities(&self) -> BTreeSet<Capability>;

    fn serve(self, links: PlatformLinks<Self::Control, Self::Backend>) -> Result<(), Self::Error>;
}

pub struct Connection<P: Platform> {
    pub platform: P,
    pub backend: P::Backend,
    pub host: HostFacts,
    pub clock: Arc<dyn Clock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFacts {
    pub outputs: Vec<OutputId>,
    pub reports_presentation: bool,
}

pub struct PlatformLinks<C: SurfaceControl, B: Backend> {
    pub reports: Reports<C, B>,
    pub requests: Receiver<PlatformRequest>,
}

pub struct Reports<C: SurfaceControl, B: Backend> {
    pub view: Outlet<PlatformEvent>,
    pub scheduler: Outlet<TimingEvent>,
    pub renderer: Outlet<SurfaceAttachment<C, B::Surface>>,
    pub streams: Outlet<StreamTask<B::Streams>>,
    pub clock: Arc<dyn Clock>,
}

impl<C: SurfaceControl, B: Backend> Reports<C, B> {
    pub fn report(&self, event: impl Into<PlatformEvent>) {
        let _stopped_view_ends_the_run = self.view.push(event.into());
    }

    pub fn time(&self, event: TimingEvent) {
        let _stopped_scheduler_ends_the_run = self.scheduler.push(event);
    }

    pub fn attach(&self, attachment: SurfaceAttachment<C, B::Surface>) {
        let _stopped_renderer_ends_the_run = self.renderer.push(attachment);
    }

    pub fn run_on_streams(&self, task: StreamTask<B::Streams>) {
        let _stopped_renderer_ends_the_run = self.streams.push(task);
    }
}

pub enum SurfaceAttachment<C: SurfaceControl, W> {
    Attach(Box<Attach<C, W>>),
    Control {
        surface: SurfaceId,
        message: C::Message,
    },
    Detach(SurfaceId),
}
