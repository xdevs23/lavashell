// SPDX-License-Identifier: GPL-3.0-only
//! The display reported by an AOSP host, and the output events that report it to the platform.

use crate::Error;
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_core::geometry::Size;
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Px};
use lavashell_platform::{OutputEvent, OutputId, Reports};
use std::mem;
use std::time::Duration;

/// The host shows the one Surface on one display.
pub(crate) const DISPLAY: OutputId = OutputId(0);

/// The display that shows the Surface. `display_of` reads it from an android.content.Context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostDisplay {
    pub width: Px,
    pub height: Px,
    pub density: Density,
    /// The time between two refreshes of the display, or `None` when the display reports no usable refresh rate.
    pub refresh: Option<Duration>,
}

/// The values that `display_of` reads through JNI.
#[derive(Clone, Copy)]
pub(crate) struct DisplayReading {
    /// The width of the largest window on the display, in pixels.
    pub(crate) width: f64,
    /// The height of the largest window on the display, in pixels.
    pub(crate) height: f64,
    pub(crate) density_dpi: f64,
    /// The refresh rate in hertz.
    pub(crate) refresh_rate: f64,
}

/// The display described by `reading`. `refresh` is `None` unless the refresh rate gives a positive interval.
///
/// # Errors
/// Fails when the dpi of `reading` is not a positive finite number.
pub(crate) fn display_from(reading: DisplayReading) -> Result<HostDisplay, Error> {
    let density = Density::from_dpi(reading.density_dpi).map_err(Error::ConvertDensity)?;
    let refresh = Duration::try_from_secs_f64(reading.refresh_rate.recip()).ok();
    Ok(HostDisplay {
        width: Px::new(reading.width),
        height: Px::new(reading.height),
        density,
        refresh: refresh.filter(|refresh| !refresh.is_zero()),
    })
}

/// The display that shows the Surface, reported to the platform as its one output.
pub(crate) struct HostOutput {
    display: HostDisplay,
}

impl HostOutput {
    /// Reports `display` as an added output.
    pub(crate) fn added(
        display: HostDisplay,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Self {
        reports.report(OutputEvent::Added(DISPLAY));
        report_geometry(display, reports);
        Self { display }
    }

    /// Reports `display` as the new state of the output, and returns its density when the density changed.
    pub(crate) fn changed(
        &mut self,
        display: HostDisplay,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Option<Density> {
        report_geometry(display, reports);
        let previous = mem::replace(&mut self.display, display);
        (previous.density != display.density).then_some(display.density)
    }

    /// Reports that `surface` is shown on the display.
    pub(crate) fn entered(surface: SurfaceId, reports: &Reports<NativeWindow, AospStreams>) {
        reports.report(OutputEvent::Entered(DISPLAY, surface));
    }

    pub(crate) fn density(&self) -> Density {
        self.display.density
    }
}

/// Reports the size and the refresh interval of `display`.
fn report_geometry(display: HostDisplay, reports: &Reports<NativeWindow, AospStreams>) {
    let monitor = MonitorInfo {
        name: None,
        size: Size::from_px(display.width, display.height, display.density),
        physical_size: None,
    };
    reports.report(OutputEvent::Updated(DISPLAY, monitor));
    reports.report(OutputEvent::Refresh(DISPLAY, display.refresh));
}
