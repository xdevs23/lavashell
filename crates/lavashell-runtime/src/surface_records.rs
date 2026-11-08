// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;

pub use accessibility::AccessibilityOutcome;

use crate::accessibility::SurfaceAccessibility;
use crate::lifecycle::Lifecycle;
use crate::settings::HostSettings;
use crate::{DragRegion, Error};
use lavashell_core::geometry::{Point, Size, Viewport};
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp};
use lavashell_core::view::{Proposal, TextMeasurer, View};
use lavashell_platform::{SettingsEvent, SurfaceEvent, SurfaceRequest};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

struct Record {
    view: View,
    /// The platform facts supplied by the app for the view, before facts reported by the host replace them.
    supplied: PlatformFacts,
    size: Size,
    density: Density,
    lifecycle: Lifecycle,
    drag_region: Option<DragRegion>,
    accessibility: SurfaceAccessibility,
}

impl Record {
    fn configure(&mut self, width: Option<Dp>, height: Option<Dp>) {
        self.size = Size::new(
            width.unwrap_or(self.size.width),
            height.unwrap_or(self.size.height),
        );
        self.lifecycle = Lifecycle::Configured;
        self.view.invalidate();
    }

    fn change_density(&mut self, density: Density) {
        self.density = density;
        self.view.invalidate();
    }
}

/// Contains a record for each surface opened by a platform until that surface retires, and the settings provided to
/// the view of every open surface. Ids come from a counter that only increases, so an id never repeats and a retired
/// id is unknown.
pub struct SurfaceRecords {
    records: BTreeMap<SurfaceId, Record>,
    last_id: Option<u64>,
    settings: HostSettings,
}

impl SurfaceRecords {
    /// Creates records without a surface. Each preference set in `overrides` replaces the preference reported by the
    /// host.
    #[must_use]
    pub fn new(overrides: PreferenceOverrides) -> Self {
        Self {
            records: BTreeMap::new(),
            last_id: None,
            settings: HostSettings::new(overrides),
        }
    }

    /// Opens a surface that shows `view` at `size` and `density` until the host configures it. The view gets the
    /// current settings. A press of the primary button in `drag_region` starts a move of the surface by the host.
    /// Assistive technologies read `accessible_name` as the name of the view when its root node is unnamed.
    ///
    /// # Errors
    ///
    /// Fails when every surface id is used.
    pub fn open(
        &mut self,
        view: View,
        size: Size,
        density: Density,
        drag_region: Option<DragRegion>,
        accessible_name: String,
    ) -> Result<SurfaceId, Error> {
        let id = match self.last_id {
            None => 0,
            Some(last) => last.checked_add(1).ok_or(Error::NumberSurface)?,
        };
        self.last_id = Some(id);
        let id = SurfaceId(id);
        let supplied = view.platform();
        self.settings.provide([(&view, &supplied)]);
        let record = Record {
            view,
            supplied,
            size,
            density,
            lifecycle: Lifecycle::Waiting,
            drag_region,
            accessibility: SurfaceAccessibility::new(accessible_name),
        };
        let _replaced_record = self.records.insert(id, record);
        Ok(id)
    }

    /// Applies `event`, which the host reported for `surface`, and returns the request that follows from it. A close
    /// returns the request to destroy the native surface, and the view is dropped later by `retire`. An event for a
    /// surface that is not open changes nothing.
    pub fn handle(&mut self, surface: SurfaceId, event: SurfaceEvent) -> Option<SurfaceRequest> {
        let record = self.open_record_mut(surface)?;
        match event {
            SurfaceEvent::Configured { width, height, .. } => record.configure(width, height),
            SurfaceEvent::Closed => {
                record.lifecycle = Lifecycle::Closing;
                return Some(SurfaceRequest::Destroy(surface));
            }
            SurfaceEvent::DensityChanged(density) => record.change_density(density),
        }
        None
    }

    /// Applies a setting reported by the host, and provides the settings to the view of every open surface.
    pub fn change_settings(&mut self, event: SettingsEvent) {
        self.settings.change(event);
        let open = (self.records.values()).filter(|record| record.lifecycle.is_open());
        self.settings
            .provide(open.map(|record| (&record.view, &record.supplied)));
    }

    /// Retires a closing `surface` once the platform has destroyed the native surface. The record is removed. The view
    /// shown by the app and every resource in it are therefore dropped only after the native surface and everything
    /// that presented to it.
    pub fn retire(&mut self, surface: SurfaceId) {
        if let Entry::Occupied(record) = self.records.entry(surface)
            && record.get().lifecycle == Lifecycle::Closing
        {
            drop(record.remove());
        }
    }

    /// The view of `surface` while it is open.
    #[must_use]
    pub fn open_view(&self, surface: SurfaceId) -> Option<&View> {
        self.open_record(surface).map(|record| &record.view)
    }

    /// The view of `surface` while it is open.
    pub fn open_view_mut(&mut self, surface: SurfaceId) -> Option<&mut View> {
        self.open_record_mut(surface).map(|record| &mut record.view)
    }

    /// The size in dp and the density of `surface` while it is open.
    #[must_use]
    pub fn open_viewport(&self, surface: SurfaceId) -> Option<Viewport> {
        (self.open_record(surface)).map(|record| Viewport::new(record.size, record.density))
    }

    /// The view of `surface` while it is open or closing.
    pub fn view_mut(&mut self, surface: SurfaceId) -> Option<&mut View> {
        (self.records.get_mut(&surface)).map(|record| &mut record.view)
    }

    /// Whether `surface` is known and has not closed.
    #[must_use]
    pub fn is_open(&self, surface: SurfaceId) -> bool {
        self.open_record(surface).is_some()
    }

    /// Whether the host configured `surface` and it has not closed since.
    #[must_use]
    pub fn is_configured(&self, surface: SurfaceId) -> bool {
        (self.records.get(&surface)).is_some_and(|record| record.lifecycle.is_configured())
    }

    /// Whether `at` in dp lies in the drag region of `surface` while it is open.
    pub(crate) fn in_drag_region(&self, surface: SurfaceId, at: Point) -> bool {
        self.open_record(surface).is_some_and(|record| {
            (record.drag_region.as_ref()).is_some_and(|region| region(at, record.size))
        })
    }

    #[must_use]
    pub fn any_open(&self) -> bool {
        (self.records.values()).any(|record| record.lifecycle.is_open())
    }

    fn open_record(&self, surface: SurfaceId) -> Option<&Record> {
        (self.records.get(&surface)).filter(|record| record.lifecycle.is_open())
    }

    fn open_record_mut(&mut self, surface: SurfaceId) -> Option<&mut Record> {
        (self.records.get_mut(&surface)).filter(|record| record.lifecycle.is_open())
    }

    /// Every open record with the id of its surface.
    fn open_records(&self) -> impl Iterator<Item = (SurfaceId, &Record)> {
        (self.records.iter())
            .filter(|(_, record)| record.lifecycle.is_open())
            .map(|(&surface, record)| (surface, record))
    }

    /// Every open record with the id of its surface.
    fn open_records_mut(&mut self) -> impl Iterator<Item = (SurfaceId, &mut Record)> {
        (self.records.iter_mut())
            .filter(|(_, record)| record.lifecycle.is_open())
            .map(|(&surface, record)| (surface, record))
    }
}

/// The size that `view` fits within `proposal`.
///
/// # Errors
///
/// Fails when the view cannot be measured.
pub fn measure_content(
    view: &mut View,
    proposal: Proposal,
    text: &mut impl TextMeasurer,
) -> Result<Size, Error> {
    (view.size_that_fits(proposal, text)).map_err(Error::MeasureView)
}

#[cfg(test)]
mod tests;
