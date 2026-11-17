// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod error;

pub use accessibility::AccessibilityFailure;
pub use error::Error;

use crate::accessibility::SurfaceAccessibility;
use crate::lifecycle::Lifecycle;
use crate::settings::HostSettings;
use crate::window::DragRegion;
use lavashell_core::geometry::{PixelScale, Point, ScaledViewport, Size, Viewport};
use lavashell_core::platform::PlatformFacts;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::surface::{SurfaceId, SurfaceIds};
use lavashell_core::units::{Density, Dp};
use lavashell_core::view::{Proposal, TextMeasurer, View};
use lavashell_core::window::WindowState;
use lavashell_platform::{SettingsEvent, SurfaceEvent, SurfaceRequest};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

struct Record {
    view: View,
    supplied: PlatformFacts,
    size: Size,
    density: Density,
    scale: PixelScale,
    lifecycle: Lifecycle,
    drag_region: Option<DragRegion>,
    accessibility: SurfaceAccessibility,
}

impl Record {
    fn configure(&mut self, width: Option<Dp>, height: Option<Dp>, window: Option<WindowState>) {
        self.size = Size::new(
            width.unwrap_or(self.size.width),
            height.unwrap_or(self.size.height),
        );
        if let Some(state) = window {
            self.view.provide_window(state);
        }
        self.lifecycle = Lifecycle::Configured;
        self.view.invalidate();
    }

    fn change_density(&mut self, density: Density) {
        self.density = density;
        self.view.invalidate();
    }

    fn change_scale(&mut self, scale: PixelScale) {
        self.scale = scale;
        self.view.invalidate();
    }

    fn scaled_viewport(&self) -> ScaledViewport {
        ScaledViewport {
            viewport: Viewport::new(self.size, self.density),
            scale: self.scale,
        }
    }
}

pub struct SurfaceRecords {
    records: BTreeMap<SurfaceId, Record>,
    surface_ids: SurfaceIds,
    settings: HostSettings,
}

impl SurfaceRecords {
    #[must_use]
    pub fn new(overrides: PreferenceOverrides) -> Self {
        Self {
            records: BTreeMap::new(),
            surface_ids: SurfaceIds::default(),
            settings: HostSettings::new(overrides),
        }
    }

    pub fn open(
        &mut self,
        view: View,
        size: Size,
        density: Density,
        drag_region: Option<DragRegion>,
        accessible_name: Option<String>,
    ) -> Result<SurfaceId, Error> {
        let id = self.surface_ids.next_id().map_err(Error::NumberSurface)?;
        let supplied = view.platform();
        self.settings.provide([(&view, &supplied)]);
        let record = Record {
            view,
            supplied,
            size,
            density,
            scale: PixelScale::SQUARE,
            lifecycle: Lifecycle::Waiting,
            drag_region,
            accessibility: SurfaceAccessibility::new(accessible_name),
        };
        let _replaced_record = self.records.insert(id, record);
        Ok(id)
    }

    pub fn handle(&mut self, surface: SurfaceId, event: SurfaceEvent) -> Option<SurfaceRequest> {
        let record = self.open_record_mut(surface)?;
        match event {
            SurfaceEvent::Configured {
                width,
                height,
                window,
                ..
            } => record.configure(width, height, window),
            SurfaceEvent::Closed => {
                record.lifecycle = Lifecycle::Closing;
                return Some(SurfaceRequest::Destroy(surface));
            }
            SurfaceEvent::DensityChanged(density) => record.change_density(density),
            SurfaceEvent::ScaleChanged(scale) => record.change_scale(scale),
        }
        None
    }

    pub fn change_settings(&mut self, event: SettingsEvent) {
        self.settings.change(event);
        let open = (self.records.values()).filter(|record| record.lifecycle.is_open());
        self.settings
            .provide(open.map(|record| (&record.view, &record.supplied)));
    }

    pub fn retire(&mut self, surface: SurfaceId) {
        if let Entry::Occupied(record) = self.records.entry(surface)
            && record.get().lifecycle == Lifecycle::Closing
        {
            drop(record.remove());
        }
    }

    #[must_use]
    pub fn open_view(&self, surface: SurfaceId) -> Option<&View> {
        self.open_record(surface).map(|record| &record.view)
    }

    pub fn open_view_mut(&mut self, surface: SurfaceId) -> Option<&mut View> {
        self.open_record_mut(surface).map(|record| &mut record.view)
    }

    #[must_use]
    pub fn open_viewport(&self, surface: SurfaceId) -> Option<Viewport> {
        (self.open_scaled_viewport(surface)).map(|scaled| scaled.viewport)
    }

    #[must_use]
    pub fn open_scaled_viewport(&self, surface: SurfaceId) -> Option<ScaledViewport> {
        self.open_record(surface).map(Record::scaled_viewport)
    }

    pub fn view_mut(&mut self, surface: SurfaceId) -> Option<&mut View> {
        (self.records.get_mut(&surface)).map(|record| &mut record.view)
    }

    #[must_use]
    pub fn is_open(&self, surface: SurfaceId) -> bool {
        self.open_record(surface).is_some()
    }

    #[must_use]
    pub fn is_configured(&self, surface: SurfaceId) -> bool {
        (self.records.get(&surface)).is_some_and(|record| record.lifecycle.is_configured())
    }

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

    fn open_records(&self) -> impl Iterator<Item = (SurfaceId, &Record)> {
        (self.records.iter())
            .filter(|(_, record)| record.lifecycle.is_open())
            .map(|(&surface, record)| (surface, record))
    }

    fn open_records_mut(&mut self) -> impl Iterator<Item = (SurfaceId, &mut Record)> {
        (self.records.iter_mut())
            .filter(|(_, record)| record.lifecycle.is_open())
            .map(|(&surface, record)| (surface, record))
    }
}

pub fn measure_content(
    view: &mut View,
    proposal: Proposal,
    text: &mut impl TextMeasurer,
) -> Result<Size, Error> {
    (view.size_that_fits(proposal, text)).map_err(Error::MeasureView)
}

#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
