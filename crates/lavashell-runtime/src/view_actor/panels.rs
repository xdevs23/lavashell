// SPDX-License-Identifier: GPL-3.0-only
use super::frames::send_surface_state;
use super::{ViewActor, send_request};
use crate::panels::{ContentState, Transaction};
use crate::scene_pipeline::FrameStatus;
use crate::scheduler::SchedulerInput;
use crate::view_actor::ViewError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{AxisPlacement, PanelGeometry};
use lavashell_core::units::{Dp, NonNegativeDp};
use lavashell_platform::{PanelRequest, Platform, PlatformRequest};
use lavashell_renderer::SurfaceContents;

/// The length at which a surface opens along an axis whose length the host sets: one logical pixel, the smallest
/// length that a host shows.
pub(super) const OPENING_LENGTH: Dp = Dp::new(1.0);

impl<P: Platform> ViewActor<P> {
    /// Sends the requests made by the runtime for each panel before a frame is rendered. While the host shows the
    /// current content of a panel, a changed geometry or keyboard mode is committed without a frame once the scene
    /// builder built it and no frame of the panel is composed or drawn.
    pub(super) fn update_panels(&mut self) -> Result<(), ViewError> {
        // A transaction whose geometry the sink sent already, such as a change of the keyboard mode alone, builds no
        // new state.
        let mut changed_states = Vec::new();
        for (&id, surface) in &mut self.surfaces {
            if self.panels.get(id).is_none() {
                continue;
            }
            let Some(viewport) = self.records.open_viewport(id) else {
                continue;
            };
            let Some(view) = self.records.open_view_mut(id) else {
                continue;
            };
            let content = if surface.needs_visual_frame(view) {
                ContentState::Stale
            } else {
                match surface.contents(viewport) {
                    SurfaceContents::Current => ContentState::Current,
                    SurfaceContents::Outdated => ContentState::Stale,
                }
            };
            let update = self.panels.update_transaction(id, content);
            if let Some(keyboard) = update.keyboard {
                let request = PanelRequest::SetKeyboard(id, keyboard);
                send_request(&self.requests, PlatformRequest::Panel(request))?;
            }
            match update.transaction {
                Transaction::Settled | Transaction::Committing => {}
                Transaction::NeedsFrame => view.invalidate(),
                Transaction::Committed => {
                    let sink = self.actors.scene.sink(id, None);
                    if send_surface_state(sink, &self.panels, id, [])? == FrameStatus::Open {
                        changed_states.push(id);
                    }
                }
            }
        }
        for id in changed_states {
            self.schedule(SchedulerInput::StateChanged(id));
        }
        Ok(())
    }
}

/// The size at which a panel with `geometry` opens, before the host configures it. An axis that fills opens at
/// [`OPENING_LENGTH`].
pub(super) fn opening_size(geometry: PanelGeometry) -> Size {
    let length = |span: AxisPlacement<NonNegativeDp>| {
        span.extent().map_or(OPENING_LENGTH, NonNegativeDp::get)
    };
    Size::new(length(geometry.horizontal()), length(geometry.vertical()))
}

/// The length of one axis of a panel configured with `configured` along it, while the panel requests `requested`
/// along it. A host that leaves the length of an axis to the panel configures `None`, and the panel takes the length
/// requested along that axis. The length is unknown for an axis requested as fill.
pub(super) fn configured_length(
    configured: Option<Dp>,
    requested: AxisPlacement<NonNegativeDp>,
) -> Option<Dp> {
    configured.or_else(|| requested.extent().map(NonNegativeDp::get))
}

#[cfg(test)]
mod tests;
