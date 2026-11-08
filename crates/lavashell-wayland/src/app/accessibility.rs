// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use crate::accessibility::Bridge;
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{AccessibilityEvent, AccessibilityRequest};
use sctk::reexports::calloop::channel::{self, Event as ChannelEvent, Sender};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;

pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<Sender<AccessibilityEvent>, Error> {
    let (sender, channel) = channel::channel();
    let source = handle
        .insert_source(channel, |event, (), platform| {
            if let ChannelEvent::Msg(event) = event {
                platform.accessibility_event(event);
            }
        })
        .map_err(|failure| Error::WatchAccessibility(failure.error))?;
    sources.push(source);
    Ok(sender)
}

impl Platform {
    /// Sends `event` to the runtime and executes the requests that the runtime returns. A deactivation first drops the
    /// tree stored for the assistive technologies.
    fn accessibility_event(&mut self, event: AccessibilityEvent) {
        let id = event.surface();
        if let AccessibilityEvent::Deactivate(_) = event
            && let Some(bridge) = self.bridge_mut(id)
        {
            bridge.deactivate();
        }
        let (surfaces, choreographers, renderer) = (
            &mut self.surfaces,
            &mut self.choreographers,
            &mut self.renderer,
        );
        let show = |view: &mut View, viewport: Viewport| -> Result<(), Error> {
            let surface = surfaces.get_mut(&id).ok_or(Error::SurfaceNotOpen(id))?;
            surface.show(id, view, choreographers, |view, sink| {
                view.frame_into(viewport, renderer.shaper(), sink)
            })
        };
        let outcome = (self.surface_records).handle_accessibility(event, show);
        for failure in outcome.failures {
            log::error!("{failure}");
        }
        self.execute_accessibility_requests(outcome.publish);
    }

    /// Sets the window focus of every surface for the assistive technologies to whether a keyboard focuses it.
    pub(super) fn update_window_focus(&mut self) {
        let focused = |surface| self.input.is_keyboard_focused(surface);
        let requests = self.surface_records.window_focus_requests(focused);
        self.execute_accessibility_requests(requests);
    }

    pub(super) fn accessibility_updates(&mut self) {
        let requests = self.surface_records.update_accessibility();
        self.execute_accessibility_requests(requests);
    }

    /// Publishes the changes of the accessibility tree of `surface` after its view showed a frame.
    pub(super) fn publish_accessibility(&mut self, surface: SurfaceId) {
        let request = self.surface_records.publish_accessibility(surface);
        self.execute_accessibility_requests(request);
    }

    fn execute_accessibility_requests(
        &mut self,
        requests: impl IntoIterator<Item = AccessibilityRequest>,
    ) {
        for request in requests {
            match request {
                AccessibilityRequest::Publish { surface, tree } => {
                    if let Some(bridge) = self.bridge_mut(surface) {
                        bridge.publish(*tree);
                    }
                }
                AccessibilityRequest::SetWindowFocus { surface, focused } => {
                    if let Some(bridge) = self.bridge_mut(surface) {
                        bridge.window_focus(focused);
                    }
                }
            }
        }
    }

    fn bridge_mut(&mut self, surface: SurfaceId) -> Option<&mut Bridge> {
        self.surfaces.get_mut(&surface)?.accessibility.as_mut()
    }
}
