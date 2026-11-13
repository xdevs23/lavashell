// SPDX-License-Identifier: GPL-3.0-only
use crate::app::Control;
use lavashell_actor::Outlet;
use lavashell_platform::{PlatformEvent, Reports, SurfaceAttachment, TimingEvent};
use std::time::Instant;

/// The links through which the translator reports to the actors of the runtime. A report to an actor that is gone is
/// dropped, because a gone actor ends the run, which ends the requests and then serving.
pub(crate) struct Links(Reports<Control>);

impl Links {
    pub(crate) fn new(reports: Reports<Control>) -> Self {
        Self(reports)
    }

    pub(crate) fn report(&self, event: impl Into<PlatformEvent>) {
        push_to_view(&self.0.view, event.into());
    }

    /// Reports `event` to the frame scheduler.
    pub(crate) fn time(&self, event: TimingEvent) {
        let _gone_scheduler_ends_the_run = self.0.scheduler.push(event);
    }

    /// Sends `attachment` to the renderer.
    pub(crate) fn attach(&self, attachment: SurfaceAttachment<Control>) {
        let _gone_renderer_ends_the_run = self.0.renderer.push(attachment);
    }

    /// The link to the view for the reports made by event sources and by other threads.
    pub(crate) fn view(&self) -> ViewLink {
        ViewLink::new(self.0.view.clone())
    }

    /// The start of the clock of the app.
    pub(crate) fn start(&self) -> Instant {
        self.0.start
    }
}

/// A link to the view, which drops a report once the view is gone.
#[derive(Clone)]
pub(crate) struct ViewLink(Outlet<PlatformEvent>);

impl ViewLink {
    pub(crate) fn new(view: Outlet<PlatformEvent>) -> Self {
        Self(view)
    }

    pub(crate) fn report(&self, event: impl Into<PlatformEvent>) {
        push_to_view(&self.0, event.into());
    }
}

/// Pushes `event` to `view`. A view that is gone ends the run, so the event is dropped.
fn push_to_view(view: &Outlet<PlatformEvent>, event: PlatformEvent) {
    let _gone_view_ends_the_run = view.push(event);
}
