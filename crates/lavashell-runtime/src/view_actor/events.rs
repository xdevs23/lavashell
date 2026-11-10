// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use super::frames::send_frame;
use super::panels::configured_length;
use crate::panels::Panel;
use crate::scheduler::SchedulerInput;
use crate::surface_records::AccessibilityFailure;
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::error::message_chain;
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::View;
use lavashell_platform::{
    AccessibilityEvent, AccessibilityRequest, OutputEvent, OutputId, PlatformEvent,
    PlatformRequest, SurfaceEvent,
};
use lavashell_renderer::SurfaceControl;

impl<C: SurfaceControl + Send + 'static> ViewActor<C> {
    pub(super) fn platform_event(&mut self, event: PlatformEvent) -> Result<(), ViewError> {
        match event {
            PlatformEvent::Surface(id, event) => self.surface_event(id, event)?,
            PlatformEvent::Output(event) => self.output_event(event)?,
            PlatformEvent::Device(event) => self.device_event(event)?,
            PlatformEvent::Settings(event) => self.records.change_settings(event),
            PlatformEvent::Watch(event) => self.transfers.watches.handle(event),
            PlatformEvent::Clipboard(event) => self.transfers.clipboard.handle(event),
            PlatformEvent::DragTarget(event) => {
                (self.transfers.drags).handle_target(event, &mut self.records);
            }
            PlatformEvent::DragSource(event) => {
                (self.transfers.drags).handle_source(event, &mut self.records);
            }
            PlatformEvent::Lock(event) => {
                if let Some(lock) = &mut self.session_lock {
                    lock.handle(event);
                }
            }
            PlatformEvent::FocusGrab(event) => self.focus_grabs.handle(event),
            PlatformEvent::Accessibility(event) => self.accessibility_event(event)?,
        }
        Ok(())
    }

    /// Hands `event` for surface `id` to the records of the views, and queues the request that follows from it. A
    /// configure with a token makes the next surface state of an open surface contain the token. A panel takes the
    /// length that it requests along an axis that the host leaves to it.
    fn surface_event(&mut self, id: SurfaceId, event: SurfaceEvent) -> Result<(), ViewError> {
        let event = match event {
            SurfaceEvent::Configured {
                width,
                height,
                token,
            } => {
                if let Some(token) = token
                    && self.records.is_open(id)
                {
                    let mut sink = self.actors.scene.sink(id, None);
                    (sink.set_state(SurfaceChange::Configure(token)))
                        .map_err(|Closed| ViewError::Stopped(Stopped::Scene))?;
                }
                let requested = self.panels.get(id).map(Panel::geometry);
                SurfaceEvent::Configured {
                    width: requested.map_or(width, |geometry| {
                        configured_length(width, geometry.horizontal())
                    }),
                    height: requested.map_or(height, |geometry| {
                        configured_length(height, geometry.vertical())
                    }),
                    token,
                }
            }
            event @ (SurfaceEvent::Closed | SurfaceEvent::DensityChanged(_)) => event,
        };
        self.surface_requests.extend(self.records.handle(id, event));
        Ok(())
    }

    /// Hands `event` to the records of the outputs. A new output gets a lock surface while the session is locked, and
    /// the panels per output follow the outputs that the host adds, changes and removes.
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
                // A surface that is not open was forgotten already, and an enter sent by the host before it saw the
                // destroy would add it again.
                if self.records.is_open(surface) {
                    self.outputs.handle(event);
                }
            }
            OutputEvent::Refresh(..) | OutputEvent::Left(..) => self.outputs.handle(event),
        }
        self.report_output_refreshes();
        Ok(())
    }

    /// Opens the panels per output on `output`, which changed.
    fn open_panels_per_output(&mut self, output: OutputId) -> Result<(), ViewError> {
        // The panels per output are moved out of the actor while they open panels, because opening a panel borrows the
        // whole actor.
        let Some(mut per_output) = self.panels_per_output.take() else {
            return Ok(());
        };
        let info = self.outputs.info(output).cloned();
        let opened = per_output.output_updated(output, info, |options, view| {
            self.open_panel(options, view, Some(output))
        });
        self.panels_per_output = Some(per_output);
        opened
    }

    /// Closes the panels per output and the lock surface on `output`, which the host removed.
    fn close_output_surfaces(&mut self, output: OutputId) {
        if let Some(per_output) = &mut self.panels_per_output {
            let closed = per_output.output_removed(output, &mut self.records);
            self.surface_requests.extend(closed);
        }
        if let Some(lock) = &mut self.session_lock
            && let Some(surface) = lock.remove_output(output)
        {
            let closed = self.records.handle(surface, SurfaceEvent::Closed);
            self.surface_requests.extend(closed);
        }
    }

    /// Reports to the scheduler the smallest refresh interval among the outputs that show each surface, while the
    /// host does not report when it presents frames.
    fn report_output_refreshes(&self) {
        if self.host.reports_presentation {
            return;
        }
        for &surface in self.surfaces.keys() {
            self.schedule(SchedulerInput::OutputRefresh {
                surface,
                refresh: self.outputs.refreshes(surface).min(),
            });
        }
    }

    /// Sends the clipboard and drag requests that the transfers of the app made since the last render.
    pub(super) fn submit_transfers(&mut self) -> Result<(), ViewError> {
        for request in self.transfers.submit(&mut self.records) {
            self.request(request)?;
        }
        Ok(())
    }

    /// Hands `event` to the records of the views, and publishes the tree that follows from it.
    fn accessibility_event(&mut self, event: AccessibilityEvent) -> Result<(), ViewError> {
        let id = event.surface();
        let (scene, shaper) = (&mut self.actors.scene, &mut self.shaper);
        let show = |view: &mut View, viewport: Viewport| {
            send_frame(view, scene.sink(id, None), |view, sink| {
                view.frame_into(viewport, shaper, sink)
            })
        };
        let outcome = self.records.handle_accessibility(event, show);
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

    /// Updates the accessibility tree of every view that changed, and publishes the trees.
    pub(super) fn accessibility_updates(&mut self) -> Result<(), ViewError> {
        let requests = self.records.update_accessibility();
        self.publish(requests)
    }

    /// Publishes the changes of the accessibility tree of `surface` after the view of `surface` showed a frame.
    pub(super) fn publish_accessibility(&mut self, surface: SurfaceId) -> Result<(), ViewError> {
        let request = self.records.publish_accessibility(surface);
        self.publish(request)
    }

    /// Sets the window focus of every surface for the assistive technologies to whether a keyboard focuses it.
    pub(super) fn update_window_focus(&self) -> Result<(), ViewError> {
        let focused = |surface| self.input.is_keyboard_focused(surface);
        self.publish(self.records.window_focus_requests(focused))
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
