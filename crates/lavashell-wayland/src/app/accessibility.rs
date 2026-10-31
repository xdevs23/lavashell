// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, LoopHandle, Platform, SceneTree, View, open_surface, sctk};
use crate::Error;
use crate::accessibility::Event;
use lavashell_core::accessibility::ActionOutcome;
use sctk::reexports::calloop::RegistrationToken;
use sctk::reexports::calloop::channel::{self, Event as ChannelEvent, Sender};

pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<Sender<Event>, Error> {
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
    fn accessibility_event(&mut self, event: Event) {
        let id = match &event {
            Event::Activate(id) | Event::Action(id, _) | Event::Deactivate(id) => *id,
        };
        let configured = self.surface_records.is_configured(id);
        // A surface that is not open ignores accessibility requests.
        let Ok((surface, view, viewport)) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)
        else {
            return;
        };
        let Some(bridge) = surface.accessibility.as_mut() else {
            return;
        };
        if matches!(event, Event::Deactivate(_)) {
            bridge.deactivate();
            view.set_accessibility_text_geometry(false);
            return;
        }
        if matches!(event, Event::Activate(_)) {
            bridge.activate();
            view.set_accessibility_text_geometry(true);
        }
        if !configured {
            // The first frame after the compositor configures the surface publishes the complete initial tree.
            return;
        }
        let mut frame = |view: &mut View, tree: &mut SceneTree| {
            view.show_frame(tree, viewport, &mut self.paint)
        };
        let shown = view.needs_visual_frame() || view.accessibility().nodes.is_empty();
        if shown {
            if let Err(error) = surface.show(view, &mut frame) {
                log::error!("{error}");
            }
            view.invalidate();
        }
        if !shown && view.needs_accessibility_update() {
            view.update_accessibility();
        }
        let tree = view.accessibility();
        if let Event::Action(_, request) = event
            && let Some(bridge) = surface.accessibility.as_mut()
            && let Some(request) = bridge.action(request, tree)
        {
            match view.accessibility_action(request) {
                Ok(ActionOutcome::Performed) => {
                    // Show the frame now, even while a frame callback of the compositor is pending, so that the changed
                    // semantics are published at once. Showing a frame updates only the scene tree. It acquires no
                    // swapchain image and does not wait for the scene queue.
                    if let Err(error) = surface.show(view, &mut frame) {
                        log::error!("{error}");
                    }
                    view.invalidate();
                }
                Ok(ActionOutcome::Ignored) => {}
                Err(error) => log::error!("{error}"),
            }
        }
        if let Some(bridge) = surface.accessibility.as_mut() {
            bridge.publish(view.accessibility(), true);
        }
    }

    pub(super) fn accessibility_focus(&mut self) {
        for (&id, surface) in &mut self.surfaces {
            let focused = self.devices.iter().any(|seat| seat.focus == Some(id));
            if let Some(view) = self.surface_records.open_view_mut(id) {
                view.input(InputEvent::WindowFocus(focused));
            }
            if let Some(bridge) = &mut surface.accessibility {
                bridge.window_focus(focused);
            }
        }
    }

    pub(super) fn accessibility_updates(&mut self) {
        for (&id, surface) in &mut self.surfaces {
            if self.surface_records.is_configured(id)
                && let Some(view) = self.surface_records.open_view_mut(id)
                && view.needs_accessibility_update()
            {
                view.update_accessibility();
                if let Some(bridge) = &mut surface.accessibility {
                    bridge.publish(view.accessibility(), false);
                }
            }
        }
    }
}
