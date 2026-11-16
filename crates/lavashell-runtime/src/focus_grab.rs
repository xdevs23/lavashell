// SPDX-License-Identifier: GPL-3.0-only
use crate::panels::{Panel, PanelRecords};
use crate::registry::Subsystem;
use crate::run::{RuntimeSetup, SurfaceSetup};
use crate::surface_records::SurfaceRecords;
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::geometry::Size;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Capability, FocusGrabEvent, FocusGrabRequest};
use std::collections::{BTreeMap, BTreeSet};

mod error;

pub use error::Error;

inventory::submit! { Subsystem { capability: Capability::FocusGrab, used: grabs_focus } }

fn grabs_focus(setup: &RuntimeSetup) -> bool {
    (setup.surfaces.iter()).any(
        |surface| matches!(surface, SurfaceSetup::Panel(layer, _) if layer.focus_grab.is_some()),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrabState {
    Released,
    Held,
    Blocked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Readiness {
    Unavailable,
    Holdable,
    Acquirable,
}

impl Readiness {
    fn of(panel: &Panel, configured: bool, size: Size) -> Self {
        if !(configured && panel.keyboard_allowed() && panel.input_enabled()) {
            Self::Unavailable
        } else if panel.keyboard_granted() && panel.content_current(size) {
            Self::Acquirable
        } else {
            Self::Holdable
        }
    }
}

struct Record {
    grab: FocusGrab,
    state: GrabState,
}

impl Record {
    fn update(&mut self, surface: SurfaceId, readiness: Readiness) -> Option<FocusGrabRequest> {
        let next = match (self.grab.is_active(), readiness, self.state) {
            (false, _, _) | (true, Readiness::Unavailable, GrabState::Held) => GrabState::Released,
            (true, Readiness::Acquirable, GrabState::Released) => GrabState::Held,
            (true, _, state) => state,
        };
        let request = match (self.state, next) {
            (GrabState::Held, GrabState::Released) => Some(FocusGrabRequest::Release(surface)),
            (GrabState::Released, GrabState::Held) => Some(FocusGrabRequest::Acquire(surface)),
            _ => None,
        };
        self.state = next;
        request
    }

    fn cleared(&mut self) {
        if self.state != GrabState::Held {
            return;
        }
        self.grab.notify_cleared();
        self.state = if self.grab.is_active() {
            GrabState::Blocked
        } else {
            GrabState::Released
        };
    }
}

pub struct FocusGrabs {
    records: BTreeMap<SurfaceId, Record>,
    offered: bool,
}

impl FocusGrabs {
    #[must_use]
    pub fn new(offered: &BTreeSet<Capability>) -> Self {
        Self {
            records: BTreeMap::new(),
            offered: offered.contains(&Capability::FocusGrab),
        }
    }

    pub fn open(&mut self, surface: SurfaceId, grab: FocusGrab) -> Result<(), Error> {
        if !self.offered {
            return Err(Error::Unsupported(surface));
        }
        let state = GrabState::Released;
        let _replaced_grab = self.records.insert(surface, Record { grab, state });
        Ok(())
    }

    #[must_use]
    pub fn forget(&mut self, surface: SurfaceId) -> Option<FocusGrabRequest> {
        let record = self.records.remove(&surface)?;
        (record.state == GrabState::Held).then_some(FocusGrabRequest::Release(surface))
    }

    #[must_use]
    pub fn update(
        &mut self,
        panels: &PanelRecords,
        surfaces: &SurfaceRecords,
    ) -> Vec<FocusGrabRequest> {
        (self.records.iter_mut())
            .filter_map(|(&surface, record)| {
                let panel = panels.get(surface)?;
                let viewport = surfaces.open_viewport(surface)?;
                let configured = surfaces.is_configured(surface);
                record.update(surface, Readiness::of(panel, configured, viewport.size))
            })
            .collect()
    }

    pub fn handle(&mut self, event: FocusGrabEvent) {
        match event {
            FocusGrabEvent::Cleared(surface) => {
                if let Some(record) = self.records.get_mut(&surface) {
                    record.cleared();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
