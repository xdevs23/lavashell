// SPDX-License-Identifier: GPL-3.0-only
use super::frames::{FrameStart, ViewSurface};
use super::panels::configured_length;
use crate::focus_grab::{self, FocusGrabs};
use crate::panels::{
    ContentState, Panel, PanelContent, PanelRecords, PanelRequests, PanelUpdate, Transaction,
};
use crate::scheduler::{SchedulerInput, ViewState};
use crate::shape::WindowShape;
use crate::surface_records::{self, SurfaceRecords};
use crate::view_actor::ViewError;
use crate::window::DragRegion;
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::{Size, Viewport};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use lavashell_core::units::Density;
use lavashell_core::view::{TextMeasurer, View};
use lavashell_platform::{
    Capability, FocusGrabEvent, FocusGrabRequest, KeyboardMode, SurfaceEvent, SurfaceRequest,
};
use lavashell_renderer::{FrameOutcome, SurfaceContents};
use std::collections::{BTreeMap, BTreeSet};
use std::mem;

pub(super) struct Opening {
    pub(super) shape: Option<WindowShape>,
    pub(super) drag_region: Option<DragRegion>,
    pub(super) accessible_name: Option<String>,
    pub(super) composition_order: i32,
}

pub(super) struct OpenSurfaces {
    records: SurfaceRecords,
    panels: PanelRecords,
    view_surfaces: BTreeMap<SurfaceId, ViewSurface>,
    focus_grabs: FocusGrabs,
    requests: Vec<SurfaceRequest>,
}

impl OpenSurfaces {
    pub(super) fn new(overrides: PreferenceOverrides, offered: &BTreeSet<Capability>) -> Self {
        Self {
            records: SurfaceRecords::new(overrides),
            panels: PanelRecords::default(),
            view_surfaces: BTreeMap::new(),
            focus_grabs: FocusGrabs::new(offered),
            requests: Vec::new(),
        }
    }

    pub(super) fn records(&self) -> &SurfaceRecords {
        &self.records
    }

    pub(super) fn records_mut(&mut self) -> &mut SurfaceRecords {
        &mut self.records
    }

    pub(super) fn open(
        &mut self,
        view: View,
        size: Size,
        opening: Opening,
    ) -> Result<SurfaceId, surface_records::Error> {
        let id = self.records.open(
            view,
            size,
            Density::ONE,
            opening.drag_region,
            opening.accessible_name,
        )?;
        let surface = ViewSurface::new(opening.shape, opening.composition_order);
        let _replaced_surface = self.view_surfaces.insert(id, surface);
        Ok(id)
    }

    pub(super) fn add_panel(
        &mut self,
        id: SurfaceId,
        panel: Panel,
        focus_grab: Option<FocusGrab>,
    ) -> Result<(), focus_grab::Error> {
        self.panels.open(id, panel);
        match focus_grab {
            Some(grab) => self.focus_grabs.open(id, grab),
            None => Ok(()),
        }
    }

    pub(super) fn handle_event(&mut self, id: SurfaceId, event: SurfaceEvent) {
        let requested = self.panels.get(id).map(Panel::geometry);
        let event = match (event, requested) {
            (
                SurfaceEvent::Configured {
                    width,
                    height,
                    token,
                    window,
                },
                Some(geometry),
            ) => SurfaceEvent::Configured {
                width: configured_length(width, geometry.horizontal()),
                height: configured_length(height, geometry.vertical()),
                token,
                window,
            },
            (event, _) => event,
        };
        self.requests.extend(self.records.handle(id, event));
    }

    pub(super) fn close_all(&mut self) {
        for &surface in self.view_surfaces.keys() {
            let closed = self.records.handle(surface, SurfaceEvent::Closed);
            self.requests.extend(closed);
        }
    }

    pub(super) fn queue_requests(&mut self, requests: impl IntoIterator<Item = SurfaceRequest>) {
        self.requests.extend(requests);
    }

    pub(super) fn take_requests(&mut self) -> Vec<SurfaceRequest> {
        mem::take(&mut self.requests)
    }

    pub(super) fn forget(&mut self, id: SurfaceId) -> Option<FocusGrabRequest> {
        self.panels.forget(id);
        let _closed_surface = self.view_surfaces.remove(&id);
        self.focus_grabs.forget(id)
    }

    pub(super) fn focus_grab_requests(&mut self) -> Vec<FocusGrabRequest> {
        self.focus_grabs.update(&self.panels, &self.records)
    }

    pub(super) fn handle_focus_grab(&mut self, event: FocusGrabEvent) {
        self.focus_grabs.handle(event);
    }

    pub(super) fn ids(&self) -> impl Iterator<Item = SurfaceId> {
        self.view_surfaces.keys().copied()
    }

    pub(super) fn composition_order(&self) -> Vec<SurfaceId> {
        let mut order: Vec<_> = (self.view_surfaces.iter())
            .map(|(&id, surface)| (surface.composition_order, id))
            .collect();
        order.sort_unstable();
        order.into_iter().map(|(_, id)| id).collect()
    }

    pub(super) fn first_configured_viewport(&self) -> Option<Viewport> {
        (self.view_surfaces.keys())
            .find(|&&id| self.records.is_configured(id))
            .and_then(|&id| self.records.open_viewport(id))
    }

    pub(super) fn wanted_frame(
        &self,
        id: SurfaceId,
        actions_queued: bool,
    ) -> Option<SchedulerInput> {
        let surface = self.view_surfaces.get(&id)?;
        let view = self.records.open_view(id)?;
        let state = if !self.records.is_configured(id) {
            ViewState::Unconfigured
        } else if surface.needs_visual_frame(view) || actions_queued {
            ViewState::Changed
        } else {
            ViewState::Unchanged
        };
        Some(SchedulerInput::Wants {
            surface: id,
            view: state,
            deadline: view.next_deadline(),
        })
    }

    pub(super) fn open_surface(
        &mut self,
        id: SurfaceId,
    ) -> Result<(&mut ViewSurface, Viewport), ViewError> {
        (self.view_surfaces.get_mut(&id))
            .zip(self.records.open_viewport(id))
            .ok_or(ViewError::SurfaceNotOpen(id))
    }

    pub(super) fn open_view(&mut self, id: SurfaceId) -> Result<(&mut View, Viewport), ViewError> {
        let viewport = self.records.open_viewport(id);
        (self.records.open_view_mut(id))
            .zip(viewport)
            .ok_or(ViewError::SurfaceNotOpen(id))
    }

    pub(super) fn open_surface_and_view(
        &mut self,
        id: SurfaceId,
    ) -> Option<(&mut ViewSurface, &mut View, Viewport)> {
        let viewport = self.records.open_viewport(id)?;
        let surface = self.view_surfaces.get_mut(&id)?;
        let view = self.records.open_view_mut(id)?;
        Some((surface, view, viewport))
    }

    pub(super) fn open_view_surface(&mut self, id: SurfaceId) -> Option<&mut ViewSurface> {
        let open = self.records.is_open(id);
        self.view_surfaces.get_mut(&id).filter(|_| open)
    }

    pub(super) fn panel_geometry_change(&self, id: SurfaceId) -> Option<SurfaceChange> {
        self.panels.geometry_change(id)
    }

    pub(super) fn panel_requests(
        &self,
        id: SurfaceId,
        state: &SurfaceState,
    ) -> Option<PanelRequests> {
        self.panels.requests(id, state)
    }

    pub(super) fn measure_panel(
        &mut self,
        id: SurfaceId,
        text: &mut impl TextMeasurer,
    ) -> Result<(), ViewError> {
        let view = (self.records.open_view_mut(id)).ok_or(ViewError::SurfaceNotOpen(id))?;
        self.panels.measure(id, view, text)
    }

    pub(super) fn update_panel(&mut self, id: SurfaceId) -> Option<KeyboardMode> {
        self.panels.update(id)
    }

    pub(super) fn panel_updates(&mut self) -> Vec<(SurfaceId, PanelUpdate)> {
        let mut updates = Vec::new();
        for (&id, surface) in &self.view_surfaces {
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
            if update.transaction == Transaction::NeedsFrame {
                view.invalidate();
            }
            updates.push((id, update));
        }
        updates
    }

    pub(super) fn committed(&mut self, id: SurfaceId) {
        let committing =
            (self.view_surfaces.get_mut(&id)).and_then(|surface| surface.committing.take());
        if let Some(requests) = committing {
            self.panels.committed(id, requests);
        }
    }

    pub(super) fn rendered(
        &mut self,
        id: SurfaceId,
        viewport: Viewport,
        contents: SurfaceContents,
        time: FrameTime,
        outcome: FrameOutcome,
    ) -> Option<FrameStart> {
        let surface = self.open_view_surface(id)?;
        let (start, panel) = surface.finish_drawing(viewport, contents, time)?;
        let input = surface.accepts_input();
        if outcome == FrameOutcome::Shown
            && let Some(requests) = panel
        {
            let content = PanelContent {
                size: viewport.size,
                input,
            };
            self.panels.shown(id, content, requests);
        }
        Some(start)
    }
}
