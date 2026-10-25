// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, LoopHandle, Platform, SceneTree, View, sctk};
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
        let index = match &event {
            Event::Activate(index) | Event::Action(index, _) | Event::Deactivate(index) => *index,
        };
        let Some(surface) = self
            .surfaces
            .get_mut(index)
            .filter(|surface| surface.lifecycle.is_open())
        else {
            return;
        };
        let Some(bridge) = surface.accessibility.as_mut() else {
            return;
        };
        if matches!(event, Event::Deactivate(_)) {
            bridge.deactivate();
            surface.view.set_accessibility_text_geometry(false);
            return;
        }
        if matches!(event, Event::Activate(_)) {
            bridge.activate();
            surface.view.set_accessibility_text_geometry(true);
        }
        if !surface.lifecycle.is_configured() {
            // The first frame after the compositor configures the surface publishes the complete initial tree.
            return;
        }
        let viewport = surface.logical_viewport();
        let mut frame = |view: &mut View, tree: &mut SceneTree| {
            view.show_frame(tree, viewport, &mut self.paint)
        };
        let shown =
            surface.view.needs_visual_frame() || surface.view.accessibility().nodes.is_empty();
        if shown {
            if let Err(error) = surface.show(&mut frame) {
                log::error!("{error}");
            }
            surface.view.invalidate();
        }
        if !shown && surface.view.needs_accessibility_update() {
            surface.view.update_accessibility();
        }
        let tree = surface.view.accessibility();
        if let Event::Action(_, request) = event
            && let Some(bridge) = surface.accessibility.as_mut()
            && let Some(request) = bridge.action(request, tree)
        {
            match surface.view.accessibility_action(request) {
                Ok(ActionOutcome::Performed) => {
                    // Show the frame now, even while a frame callback of the compositor is pending, so that the changed
                    // semantics are published at once. Showing a frame updates only the scene tree. It acquires no
                    // swapchain image and does not wait for the scene queue.
                    if let Err(error) = surface.show(&mut frame) {
                        log::error!("{error}");
                    }
                    surface.view.invalidate();
                }
                Ok(ActionOutcome::Ignored) => {}
                Err(error) => log::error!("{error}"),
            }
        }
        if let Some(bridge) = surface.accessibility.as_mut() {
            bridge.publish(surface.view.accessibility(), true);
        }
    }

    pub(super) fn accessibility_focus(&mut self) {
        for (index, surface) in self.surfaces.iter_mut().enumerate() {
            let focused = self.devices.iter().any(|seat| seat.focus == Some(index));
            surface.view.input(InputEvent::WindowFocus(focused));
            if let Some(bridge) = &mut surface.accessibility {
                bridge.window_focus(focused);
            }
        }
    }

    pub(super) fn accessibility_updates(&mut self) {
        for surface in self
            .surfaces
            .iter_mut()
            .filter(|surface| surface.lifecycle.is_configured())
        {
            if surface.view.needs_accessibility_update() {
                surface.view.update_accessibility();
                if let Some(bridge) = &mut surface.accessibility {
                    bridge.publish(surface.view.accessibility(), false);
                }
            }
        }
    }
}
