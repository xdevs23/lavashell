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

/// Whether the host has the focus grab of a panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GrabState {
    Released,
    Held,
    /// The host cleared the grab while the panel was still active. The panel can acquire a new grab only after it
    /// becomes inactive.
    Blocked,
}

/// What a panel can do with the focus grab registered for it in an update.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Readiness {
    /// The panel releases its grab, if it has one.
    Unavailable,
    /// The panel neither releases its current grab nor acquires a new one.
    Holdable,
    /// The panel acquires a grab when it has none.
    Acquirable,
}

impl Readiness {
    /// What `panel` can do with the focus grab registered for it, while its surface is `configured` or not and has
    /// `size`.
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
    /// Moves the grab of `surface` to the state that follows from `readiness` and whether the panel is active, and
    /// returns the request needed by the move.
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

    /// Handles the host clearing the grab. A grab that is not held was released before the host cleared it.
    fn cleared(&mut self) {
        if self.state != GrabState::Held {
            return;
        }
        self.grab.notify_cleared();
        // `is_active` is read after the callback, because the callback can make the panel inactive. A panel that is
        // still active is blocked until it becomes inactive, so a new grab cannot follow the cleared one at once.
        self.state = if self.grab.is_active() {
            GrabState::Blocked
        } else {
            GrabState::Released
        };
    }
}

/// The focus grab of every panel that has one, by the id of its surface.
pub struct FocusGrabs {
    records: BTreeMap<SurfaceId, Record>,
    offered: bool,
}

impl FocusGrabs {
    /// The focus grabs on a platform that offers `offered`.
    #[must_use]
    pub fn new(offered: &BTreeSet<Capability>) -> Self {
        Self {
            records: BTreeMap::new(),
            offered: offered.contains(&Capability::FocusGrab),
        }
    }

    /// An update acquires the grab for the panel once the panel is ready.
    ///
    /// # Errors
    ///
    /// Fails when the platform does not offer focus grabs, which the setup check cannot see for a panel made per
    /// output.
    pub fn open(&mut self, surface: SurfaceId, grab: FocusGrab) -> Result<(), Error> {
        if !self.offered {
            return Err(Error::Unsupported(surface));
        }
        let state = GrabState::Released;
        let _replaced_grab = self.records.insert(surface, Record { grab, state });
        Ok(())
    }

    /// Removes the focus grab of `surface` after the surface has retired.
    #[must_use]
    pub fn forget(&mut self, surface: SurfaceId) -> Option<FocusGrabRequest> {
        let record = self.records.remove(&surface)?;
        (record.state == GrabState::Held).then_some(FocusGrabRequest::Release(surface))
    }

    /// Updates every grab from the panel and the surface it belongs to, and returns the requests that follow, in the
    /// order of the surface ids. The platform calls it after each render. A grab whose panel or open surface is
    /// missing changes nothing.
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

    /// An event for a surface without a grab changes nothing.
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
