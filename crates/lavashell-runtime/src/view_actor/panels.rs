// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use super::frames::send_surface_state;
use crate::panels::Transaction;
use crate::rendered_platform::RenderedPlatform;
use crate::scene_pipeline::FrameStatus;
use crate::scheduler::SchedulerInput;
use crate::view_actor::ViewError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{AxisPlacement, PanelGeometry};
use lavashell_core::units::{Dp, NonNegativeDp};
use lavashell_platform::{PanelRequest, PlatformRequest};

pub(super) const OPENING_LENGTH: Dp = Dp::new(1.0);

impl<P: RenderedPlatform> ViewActor<P> {
    pub(super) fn update_panels(&mut self) -> Result<(), ViewError> {
        let mut changed_states = Vec::new();
        for (id, update) in self.surfaces.panel_updates() {
            if let Some(keyboard) = update.keyboard {
                let request = PanelRequest::SetKeyboard(id, keyboard);
                self.request(PlatformRequest::Panel(request))?;
            }
            if update.transaction == Transaction::Committed {
                let sink = self.actors.scene.sink(id, None);
                if send_surface_state(sink, &self.surfaces, id, [])? == FrameStatus::Open {
                    changed_states.push(id);
                }
            }
        }
        for id in changed_states {
            self.schedule(SchedulerInput::StateChanged(id));
        }
        Ok(())
    }
}

pub(super) fn opening_size(geometry: PanelGeometry) -> Size {
    let length = |span: AxisPlacement<NonNegativeDp>| {
        span.extent().map_or(OPENING_LENGTH, NonNegativeDp::get)
    };
    Size::new(length(geometry.horizontal()), length(geometry.vertical()))
}

pub(super) fn configured_length(
    configured: Option<Dp>,
    requested: AxisPlacement<NonNegativeDp>,
) -> Option<Dp> {
    configured.or_else(|| requested.extent().map(NonNegativeDp::get))
}

#[cfg(test)]
mod tests;
