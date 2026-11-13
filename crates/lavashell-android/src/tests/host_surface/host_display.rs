// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Host, SURFACE, Sent, added, configured, create, display, displayed, showing, shown, window,
};
use crate::Error;
use crate::input::TouchAction;
use crate::output::{DISPLAY, DisplayReading, HostDisplay, display_from};
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::tests::motion::{finger, touch};
use lavashell_core::output::MonitorInfo;
use lavashell_core::prelude::{Density, DpExt, Point, PxExt, Size};
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::view::TouchSample;
use lavashell_platform::{DeviceEvent, OutputEvent, SurfaceEvent, SurfaceRequest};
use std::time::Duration;

/// Two pixels per dp.
fn double() -> Result<Density, TestError> {
    Ok(Density::from_ratio(2.0, 1.0)?)
}

/// A reading of a display of 1080 × 2400 pixels at `density_dpi` and `refresh_rate`.
fn reading(density_dpi: f64, refresh_rate: f64) -> DisplayReading {
    DisplayReading {
        width: 1080.0,
        height: 2400.0,
        density_dpi,
        refresh_rate,
    }
}

#[test]
fn a_display_reading_converts_the_dpi_and_the_refresh_rate() -> Result<(), TestError> {
    let converted = HostDisplay {
        width: 1080.px(),
        height: 2400.px(),
        density: Density::from_dpi(320.0)?,
        refresh: Some(Duration::from_millis(20)),
    };
    assert_eq!(display_from(reading(320.0, 50.0))?, converted);
    Ok(())
}

#[test]
fn a_refresh_rate_without_a_positive_interval_is_left_out() -> Result<(), TestError> {
    for refresh_rate in [0.0, -60.0, f64::NAN, f64::INFINITY] {
        assert_eq!(display_from(reading(160.0, refresh_rate))?.refresh, None);
    }
    Ok(())
}

#[test]
fn a_display_reading_without_a_positive_dpi_fails() {
    for density_dpi in [0.0, -160.0, f64::NAN] {
        let converted = display_from(reading(density_dpi, 60.0));
        assert!(matches!(converted, Err(Error::ConvertDensity(_))));
    }
}

#[test]
fn a_display_report_adds_the_output_once_and_updates_it_after() -> Result<(), TestError> {
    let mut host = Host::new(display(double()?));
    assert_eq!(host.sent()?, added(SIDE / 2));
    host.display_changed(display(Density::ONE));
    let [_, updated, refresh] = added(SIDE);
    assert_eq!(host.sent()?, [updated, refresh]);
    Ok(())
}

#[test]
fn a_surface_is_shown_at_the_density_of_the_display() -> Result<(), TestError> {
    let mut host = displayed(display(double()?))?;
    host.execute(create(SURFACE))?;
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    let shown = showing(SURFACE, SIDE / 2, SIDE / 2, double()?);
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_density_change_converts_the_size_and_touches_of_the_surface() -> Result<(), TestError> {
    let mut host = shown()?;
    host.display_changed(display(double()?));
    host.motion(touch(0, TouchAction::Down(finger(0, 8.0, 4.0))));
    let monitor = MonitorInfo {
        name: None,
        size: Size::new((SIDE / 2).dp(), (SIDE / 2).dp()),
        physical_size: None,
    };
    let [_, _, refresh] = added(SIDE);
    let touched = TouchSample {
        id: TouchId { seat: 0, point: 0 },
        at: Point::new(4.dp(), 2.dp()),
        kind: PointerKind::Finger,
    };
    let reported = [
        Sent::Output(OutputEvent::Updated(DISPLAY, monitor)),
        refresh,
        Sent::Surface(SURFACE, SurfaceEvent::DensityChanged(double()?)),
        configured(SURFACE, SIDE / 2, SIDE / 2),
        Sent::Device(DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: touched,
        }),
    ];
    assert_eq!(host.sent()?, reported);
    Ok(())
}

#[test]
fn a_display_change_reaches_only_a_shown_surface_at_another_density() -> Result<(), TestError> {
    let mut same = shown()?;
    same.display_changed(display(Density::ONE));
    let [_, updated, refresh] = added(SIDE);
    assert_eq!(same.sent()?, [updated, refresh]);

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    requested.display_changed(display(double()?));
    let [_, updated, refresh] = added(SIDE / 2);
    assert_eq!(requested.sent()?, [updated, refresh]);

    let mut closing = shown()?;
    closing.execute(SurfaceRequest::Destroy(SURFACE))?;
    let _detached = closing.sent()?;
    closing.display_changed(display(double()?));
    let [_, updated, refresh] = added(SIDE / 2);
    assert_eq!(closing.sent()?, [updated, refresh]);
    Ok(())
}
