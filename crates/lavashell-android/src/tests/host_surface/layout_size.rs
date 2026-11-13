// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Host, SURFACE, Sent, added, create, display, displayed, laid_out, showing, shown, window,
};
use crate::host::{AospError, LayoutSize};
use crate::input::TouchAction;
use crate::output::DISPLAY;
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::tests::motion::{finger, touch};
use lavashell_core::prelude::{Density, DpExt, PixelScale, PixelSize, Point, Size, Viewport};
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::view::TouchSample;
use lavashell_platform::{DeviceEvent, OutputEvent, SurfaceEvent};

/// The logical size of a car display.
fn car_size() -> Size {
    Size::new(800.dp(), 480.dp())
}

fn layout_size() -> Result<LayoutSize, TestError> {
    Ok(LayoutSize::try_new(car_size())?)
}

fn double() -> Result<Density, TestError> {
    Ok(Density::from_ratio(2.0, 1.0)?)
}

/// The scale at which `car_size()` at `density` fills `window`.
fn filling(window: PixelSize, density: Density) -> Result<PixelScale, TestError> {
    let viewport = Viewport::new(car_size(), density);
    Ok(PixelScale::filling(viewport, window)?)
}

/// The reports sent when `SURFACE` is shown in a Surface of `window` pixels, laid out in `car_size()` at one dp per
/// pixel.
fn showing_car_size(window: PixelSize) -> Result<Vec<Sent>, TestError> {
    let mut shown = vec![
        Sent::Attach(SURFACE),
        Sent::Surface(SURFACE, SurfaceEvent::DensityChanged(Density::ONE)),
    ];
    let scale = filling(window, Density::ONE)?;
    shown.extend(laid_out(SURFACE, scale, 800, 480));
    shown.push(Sent::Output(OutputEvent::Entered(DISPLAY, SURFACE)));
    Ok(shown)
}

/// A host that shows `SURFACE` in `surface`, laid out in `car_size()` at one dp per pixel. The reports of the show
/// are drained from `sent`.
fn shown_in_car_size(surface: &DisplaySurface) -> Result<Host, TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.set_layout_size(Some(layout_size()?))?;
    host.created(window(surface))?;
    host.execute(create(SURFACE))?;
    let _shown = host.sent()?;
    Ok(host)
}

/// The size in pixels of the frames of the shown surface.
fn frame_size(host: &Host) -> Result<Option<PixelSize>, TestError> {
    let Some(shown) = host.surface.shown() else {
        return Ok(None);
    };
    Ok(Some(shown.layout.scale.buffer_size(shown.layout.viewport)?))
}

/// A finger of the touch screen at `x`, `y` dp.
fn touched(x: f64, y: f64) -> TouchSample {
    TouchSample {
        id: TouchId { seat: 0, point: 0 },
        at: Point::new(x.dp(), y.dp()),
        kind: PointerKind::Finger,
    }
}

fn touch_down(sample: TouchSample) -> Sent {
    Sent::Device(DeviceEvent::TouchDown {
        surface: SURFACE,
        sample,
    })
}

#[test]
fn a_layout_size_without_a_finite_side_above_zero_is_refused() {
    let sides = [
        (0.0, 480.0),
        (800.0, -480.0),
        (f64::NAN, 480.0),
        (800.0, f64::INFINITY),
    ];
    for (width, height) in sides {
        let size = Size::new(width.dp(), height.dp());
        assert!(LayoutSize::try_new(size).is_err());
    }
}

#[test]
fn a_layout_size_set_before_the_show_lays_the_surface_out() -> Result<(), TestError> {
    let window_size = PixelSize::new(1280, 480);
    let mut absent = displayed(display(Density::ONE))?;
    absent.set_layout_size(Some(layout_size()?))?;
    absent.created(window(&DisplaySurface::new(1280, 480)))?;
    absent.execute(create(SURFACE))?;
    assert_eq!(absent.sent()?, showing_car_size(window_size)?);
    assert_eq!(frame_size(&absent)?, Some(window_size));

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    requested.set_layout_size(Some(layout_size()?))?;
    assert_eq!(requested.sent()?, []);
    requested.created(window(&DisplaySurface::new(1280, 480)))?;
    assert_eq!(requested.sent()?, showing_car_size(window_size)?);
    Ok(())
}

#[test]
fn a_touch_on_a_layout_size_reaches_the_dp_point_shown_under_it() -> Result<(), TestError> {
    let mut host = shown_in_car_size(&DisplaySurface::new(1280, 480))?;
    host.motion(touch(0, TouchAction::Down(finger(0, 793.0, 216.0))));
    assert_eq!(host.sent()?, [touch_down(touched(495.625, 216.0))]);
    Ok(())
}

#[test]
fn a_window_resize_derives_a_new_scale_for_the_layout_size() -> Result<(), TestError> {
    let surface = DisplaySurface::new(1280, 480);
    let mut host = shown_in_car_size(&surface)?;
    surface.resize(1600, 480);
    host.changed()?;
    let window_size = PixelSize::new(1600, 480);
    let rescaled = laid_out(SURFACE, filling(window_size, Density::ONE)?, 800, 480);
    assert_eq!(host.sent()?, rescaled);
    assert_eq!(frame_size(&host)?, Some(window_size));
    Ok(())
}

#[test]
fn a_density_change_lays_the_layout_size_out_at_the_new_density() -> Result<(), TestError> {
    let mut host = shown_in_car_size(&DisplaySurface::new(1280, 480))?;
    host.display_changed(display(double()?))?;
    host.motion(touch(0, TouchAction::Down(finger(0, 793.0, 216.0))));
    let [_, updated, refresh] = added(SIDE / 2);
    let window_size = PixelSize::new(1280, 480);
    let mut reported = vec![
        updated,
        refresh,
        Sent::Surface(SURFACE, SurfaceEvent::DensityChanged(double()?)),
    ];
    let scale = filling(window_size, double()?)?;
    reported.extend(laid_out(SURFACE, scale, 800, 480));
    reported.push(touch_down(touched(495.625, 216.0)));
    assert_eq!(host.sent()?, reported);
    assert_eq!(frame_size(&host)?, Some(window_size));
    Ok(())
}

#[test]
fn touches_on_a_window_without_pixels_use_the_last_layout() -> Result<(), TestError> {
    let surface = DisplaySurface::new(1280, 480);
    let mut host = shown_in_car_size(&surface)?;
    host.motion(touch(0, TouchAction::Down(finger(0, 793.0, 216.0))));
    surface.resize(0, 0);
    host.changed()?;
    host.motion(touch(0, TouchAction::Up(finger(0, 793.0, 216.0))));
    let up = Sent::Device(DeviceEvent::TouchUp(touched(495.625, 216.0)));
    assert_eq!(host.sent()?, [touch_down(touched(495.625, 216.0)), up]);
    surface.resize(1600, 480);
    host.changed()?;
    let window_size = PixelSize::new(1600, 480);
    let laid_out_again = laid_out(SURFACE, filling(window_size, Density::ONE)?, 800, 480);
    assert_eq!(host.sent()?, laid_out_again);
    Ok(())
}

#[test]
fn a_host_surface_without_pixels_is_laid_out_once_it_has_some() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    let surface = DisplaySurface::new(0, 0);
    host.created(window(&surface))?;
    host.changed()?;
    host.execute(create(SURFACE))?;
    let unconfigured = [
        Sent::Attach(SURFACE),
        Sent::Surface(SURFACE, SurfaceEvent::DensityChanged(Density::ONE)),
        Sent::Output(OutputEvent::Entered(DISPLAY, SURFACE)),
    ];
    assert_eq!(host.sent()?, unconfigured);
    host.motion(touch(0, TouchAction::Down(finger(0, 0.0, 0.0))));
    assert_eq!(host.sent()?, []);
    surface.resize(SIDE, SIDE / 2);
    host.changed()?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, SIDE, SIDE / 2);
    assert_eq!(host.sent()?, square);
    Ok(())
}

#[test]
fn without_a_layout_size_a_resized_window_has_square_pixels() -> Result<(), TestError> {
    let surface = DisplaySurface::new(SIDE, SIDE);
    let mut host = displayed(display(Density::ONE))?;
    host.created(window(&surface))?;
    host.execute(create(SURFACE))?;
    assert_eq!(host.sent()?, showing(SURFACE, SIDE, SIDE, Density::ONE));
    surface.resize(2 * SIDE, SIDE);
    host.changed()?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, 2 * SIDE, SIDE);
    assert_eq!(host.sent()?, square);
    Ok(())
}

#[test]
fn a_cleared_layout_size_lays_the_surface_out_in_the_whole_window() -> Result<(), TestError> {
    let mut host = shown_in_car_size(&DisplaySurface::new(1280, 480))?;
    host.set_layout_size(None)?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, 1280, 480);
    assert_eq!(host.sent()?, square);
    assert_eq!(frame_size(&host)?, Some(PixelSize::new(1280, 480)));
    Ok(())
}

#[test]
fn a_layout_size_too_long_at_the_density_fails_the_platform() -> Result<(), TestError> {
    let mut host = shown()?;
    host.display_changed(display(double()?))?;
    let too_long = LayoutSize::try_new(Size::new(f64::MAX.dp(), 480.dp()))?;
    let set = host.set_layout_size(Some(too_long));
    assert!(matches!(set, Err(AospError::FitLayout(_))));
    Ok(())
}
