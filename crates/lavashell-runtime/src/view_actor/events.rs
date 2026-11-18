// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use super::frames::send_frame;
use crate::scheduler::SchedulerInput;
use crate::surface_records::AccessibilityFailure;
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::View;
use lavashell_error_chain::message_chain;
use lavashell_platform::{
    AccessibilityEvent, AccessibilityRequest, OutputEvent, OutputId, Platform, PlatformEvent,
    PlatformRequest, SurfaceEvent,
};

impl<P: Platform> ViewActor<P> {
    pub(super) fn platform_event(&mut self, event: PlatformEvent) -> Result<(), ViewError> {
        match event {
            PlatformEvent::Surface(id, event) => self.surface_event(id, event)?,
            PlatformEvent::Output(event) => self.output_event(event)?,
            PlatformEvent::Device(event) => self.device_event(event)?,
            PlatformEvent::Settings(event) => self.surfaces.records_mut().change_settings(event),
            PlatformEvent::Watch(event) => self.transfers.watches.handle(event),
            PlatformEvent::Clipboard(event) => self.transfers.clipboard.handle(event),
            PlatformEvent::DragTarget(event) => {
                (self.transfers.drags).handle_target(event, self.surfaces.records_mut());
            }
            PlatformEvent::DragSource(event) => {
                (self.transfers.drags).handle_source(event, self.surfaces.records_mut());
            }
            PlatformEvent::Lock(event) => self.outputs.handle_lock(event),
            PlatformEvent::FocusGrab(event) => self.surfaces.handle_focus_grab(event),
            PlatformEvent::Accessibility(event) => self.accessibility_event(event)?,
        }
        Ok(())
    }

    fn surface_event(&mut self, id: SurfaceId, event: SurfaceEvent) -> Result<(), ViewError> {
        if let SurfaceEvent::Configured {
            token: Some(token), ..
        } = event
            && self.surfaces.records().is_open(id)
        {
            let mut sink = self.actors.scene.sink(id, None);
            (sink.set_state(SurfaceChange::Configure(token)))
                .map_err(|Closed| ViewError::Stopped(Stopped::Scene))?;
        }
        self.surfaces.handle_event(id, event);
        Ok(())
    }

    fn output_event(&mut self, event: OutputEvent) -> Result<(), ViewError> {
        match event {
            OutputEvent::Added(output) => {
                self.outputs.handle(event);
                self.open_lock_surface(output)?;
            }
            OutputEvent::Updated(output, _) => {
                self.outputs.handle(event);
                self.open_panels_per_output(output)?;
            }
            OutputEvent::Removed(output) => {
                self.close_output_surfaces(output);
                self.outputs.handle(event);
            }
            OutputEvent::Entered(_, surface) => {
                if self.surfaces.records().is_open(surface) {
                    self.outputs.handle(event);
                }
            }
            OutputEvent::Refresh(..) | OutputEvent::Left(..) => self.outputs.handle(event),
        }
        self.report_output_refreshes();
        Ok(())
    }

    fn open_panels_per_output(&mut self, output: OutputId) -> Result<(), ViewError> {
        let Some(mut per_output) = self.outputs.take_panels_per_output() else {
            return Ok(());
        };
        let info = self.outputs.info(output);
        let opened = per_output.output_updated(output, info, |options, view| {
            self.open_panel(options, view, Some(output))
        });
        self.outputs.put_back_panels_per_output(per_output);
        opened
    }

    fn close_output_surfaces(&mut self, output: OutputId) {
        let closed = (self.outputs).close_surfaces_of(output, self.surfaces.records_mut());
        self.surfaces.queue_requests(closed);
    }

    fn report_output_refreshes(&self) {
        if self.outputs.reports_presentation() && self.actors.renderer().is_some() {
            return;
        }
        for surface in self.surfaces.ids() {
            self.schedule(SchedulerInput::OutputRefresh {
                surface,
                refresh: self.outputs.shortest_refresh(surface),
            });
        }
    }

    pub(super) fn submit_transfers(&mut self) -> Result<(), ViewError> {
        for request in self.transfers.submit(self.surfaces.records_mut()) {
            self.request(request)?;
        }
        Ok(())
    }

    fn accessibility_event(&mut self, event: AccessibilityEvent) -> Result<(), ViewError> {
        let id = event.surface();
        let (scene, shaper) = (&mut self.actors.scene, &mut self.shaper);
        let observers = &mut self.observers;
        let show = |view: &mut View, viewport: Viewport| {
            let failed = |failures| observers.report_failures(id, failures);
            send_frame(view, scene.sink(id, None), failed, |view, sink| {
                view.frame_into(viewport, shaper, sink)
            })
        };
        let outcome = self
            .surfaces
            .records_mut()
            .handle_accessibility(event, show);
        for failure in outcome.failures {
            match failure {
                AccessibilityFailure::Show(ViewError::Stopped(Stopped::Scene)) => {
                    return Err(ViewError::Stopped(Stopped::Scene));
                }
                failure => log::error!("{}", message_chain(failure)),
            }
        }
        self.publish(outcome.publish)
    }

    pub(super) fn accessibility_updates(&mut self) -> Result<(), ViewError> {
        let requests = self.surfaces.records_mut().update_accessibility()?;
        self.publish(requests)
    }

    pub(super) fn publish_accessibility(&mut self, surface: SurfaceId) -> Result<(), ViewError> {
        let request = self.surfaces.records_mut().publish_accessibility(surface)?;
        self.publish(request)
    }

    pub(super) fn update_window_focus(&self) -> Result<(), ViewError> {
        let focused = |surface| self.input.is_keyboard_focused(surface);
        self.publish(self.surfaces.records().window_focus_requests(focused))
    }

    fn publish(
        &self,
        requests: impl IntoIterator<Item = AccessibilityRequest>,
    ) -> Result<(), ViewError> {
        if !self.accessibility {
            return Ok(());
        }
        for request in requests {
            self.request(PlatformRequest::Accessibility(request))?;
        }
        Ok(())
    }
}
