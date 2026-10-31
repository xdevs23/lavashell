// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::lifecycle::Lifecycle;
use lavashell_core::geometry::{Size, Viewport};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp};
use lavashell_core::view::{Proposal, TextMeasurer, View};
use lavashell_platform::{SurfaceEvent, SurfaceRequest};

struct Record {
    view: View,
    size: Size,
    density: Density,
    lifecycle: Lifecycle,
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

/// The record of every surface opened by a platform. The id of a surface is the index of its record. A retired
/// surface still has its record, so no id is reused.
#[derive(Default)]
pub struct SurfaceRecords {
    records: Vec<Record>,
}

impl SurfaceRecords {
    /// Opens a surface that shows `view` at `size` and `density` until the host configures it.
    ///
    /// # Errors
    ///
    /// Fails when the number of the new surface does not fit a surface id.
    pub fn open(&mut self, view: View, size: Size, density: Density) -> Result<SurfaceId, Error> {
        let id = SurfaceId(u64::try_from(self.records.len()).map_err(Error::NumberSurface)?);
        self.records.push(Record {
            view,
            size,
            density,
            lifecycle: Lifecycle::Waiting,
        });
        Ok(id)
    }

    /// Applies `event`, which the host reported for `surface`, and returns the request that follows from it. A close
    /// returns the request to destroy the native surface, and the view is dropped later by `retire`. An event for a
    /// surface that is not open changes nothing.
    pub fn handle(&mut self, surface: SurfaceId, event: SurfaceEvent) -> Option<SurfaceRequest> {
        let record = self.open_record_mut(surface)?;
        match event {
            SurfaceEvent::Configured { width, height } => record.configure(width, height),
            SurfaceEvent::Closed => {
                record.lifecycle = Lifecycle::Closing;
                return Some(SurfaceRequest::Destroy(surface));
            }
            SurfaceEvent::DensityChanged(density) => record.change_density(density),
        }
        None
    }

    /// Retires a closing `surface` once the platform destroyed its native surface. The view is replaced with an
    /// empty view, so the view shown by the app and every resource in it are dropped only after the native surface
    /// and everything that presented to it.
    pub fn retire(&mut self, surface: SurfaceId) {
        if let Some(record) = self.record_mut(surface)
            && record.lifecycle == Lifecycle::Closing
        {
            record.view = View::new(record.view.platform(), |_| {});
            record.lifecycle = Lifecycle::Retired;
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

    /// The view of `surface` in any state. It is the view shown by the app until the surface retired, and an empty
    /// view after that.
    pub fn view_mut(&mut self, surface: SurfaceId) -> Option<&mut View> {
        self.record_mut(surface).map(|record| &mut record.view)
    }

    /// Whether `surface` is known and has not closed.
    #[must_use]
    pub fn is_open(&self, surface: SurfaceId) -> bool {
        self.open_record(surface).is_some()
    }

    /// Whether the host configured `surface` and it has not closed since.
    #[must_use]
    pub fn is_configured(&self, surface: SurfaceId) -> bool {
        self.record(surface)
            .is_some_and(|record| record.lifecycle.is_configured())
    }

    #[must_use]
    pub fn any_open(&self) -> bool {
        (self.records.iter()).any(|record| record.lifecycle.is_open())
    }

    fn record(&self, surface: SurfaceId) -> Option<&Record> {
        self.records.get(usize::try_from(surface.0).ok()?)
    }

    fn record_mut(&mut self, surface: SurfaceId) -> Option<&mut Record> {
        self.records.get_mut(usize::try_from(surface.0).ok()?)
    }

    fn open_record(&self, surface: SurfaceId) -> Option<&Record> {
        (self.record(surface)).filter(|record| record.lifecycle.is_open())
    }

    fn open_record_mut(&mut self, surface: SurfaceId) -> Option<&mut Record> {
        (self.record_mut(surface)).filter(|record| record.lifecycle.is_open())
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

/// The size at which a window opens: `requested`, or else the size that `view` fits without bounds.
///
/// # Errors
///
/// Fails when the view cannot be measured.
pub fn initial_size(
    requested: Option<Size>,
    view: &mut View,
    text: &mut impl TextMeasurer,
) -> Result<Size, Error> {
    let unbounded = Proposal {
        width: None,
        height: None,
    };
    match requested {
        Some(size) => Ok(size),
        None => measure_content(view, unbounded, text),
    }
}

#[cfg(test)]
mod tests;
