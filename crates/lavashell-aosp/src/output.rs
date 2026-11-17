// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_core::geometry::Size;
use lavashell_core::output::MonitorInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Px, RefreshRate};
use lavashell_platform::{OutputEvent, OutputId, Reports};
use std::time::Duration;

pub(crate) const DISPLAY: OutputId = OutputId(0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostDisplay {
    pub width: Px,
    pub height: Px,
    pub density: Density,
    pub refresh: Option<Duration>,
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayReading {
    pub(crate) width: Px,
    pub(crate) height: Px,
    pub(crate) density_dpi: f64,
    pub(crate) refresh_rate: RefreshRate,
}

impl TryFrom<DisplayReading> for HostDisplay {
    type Error = Error;

    fn try_from(reading: DisplayReading) -> Result<Self, Error> {
        let density = Density::from_dpi(reading.density_dpi).map_err(Error::ConvertDensity)?;
        Ok(Self {
            width: reading.width,
            height: reading.height,
            density,
            refresh: reading.refresh_rate.interval(),
        })
    }
}

pub(crate) fn report_added(display: HostDisplay, reports: &Reports<NativeWindow, AospStreams>) {
    reports.report(OutputEvent::Added(DISPLAY));
    report_display(display, reports);
}

pub(crate) fn report_entered(surface: SurfaceId, reports: &Reports<NativeWindow, AospStreams>) {
    reports.report(OutputEvent::Entered(DISPLAY, surface));
}

pub(crate) fn report_display(display: HostDisplay, reports: &Reports<NativeWindow, AospStreams>) {
    let monitor = MonitorInfo {
        name: None,
        size: Size::from_px(display.width, display.height, display.density),
        physical_size: None,
    };
    reports.report(OutputEvent::Updated(DISPLAY, monitor));
    reports.report(OutputEvent::Refresh(DISPLAY, display.refresh));
}
