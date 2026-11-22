// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Host, SURFACE, Sent, create, display, displayed, laid_out, showing, shown, touch_down, touched,
    two_pixels_per_dp, updated, window,
};
use crate::host::AospError;
use crate::input::{MotionEvent, TouchAction};
use crate::layout_size::LayoutSize;
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::tests::motion::finger;
use lavashell_core::prelude::{Density, DpExt, PixelScale, PixelSize, Size, Viewport};
use lavashell_platform::{DeviceEvent, SurfaceEvent};

fn car_display_size() -> Size {
    Size::new(800.dp(), 480.dp())
}

fn car_layout_size() -> Result<LayoutSize, TestError> {
    Ok(LayoutSize::try_new(car_display_size())?)
}

fn car_display_filling(window: PixelSize, density: Density) -> Result<PixelScale, TestError> {
    let viewport = Viewport::new(car_display_size(), density);
    Ok(PixelScale::filling(viewport, window)?)
}

fn shown_in_car_layout(surface: &DisplaySurface) -> Result<Host, TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.set_layout_size(Some(car_layout_size()?))?;
    host.created(window(surface))?;
    host.execute(create(SURFACE))?;
    let _shown = host.sent()?;
    Ok(host)
}

fn frame_size(host: &Host) -> Result<Option<PixelSize>, TestError> {
    let Some(shown) = host.state.surface.input_target() else {
        return Ok(None);
    };
    Ok(Some(shown.scaled_viewport.buffer_size()?))
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
    absent.set_layout_size(Some(car_layout_size()?))?;
    absent.created(window(&DisplaySurface::new(1280, 480)))?;
    absent.execute(create(SURFACE))?;
    let scale = car_display_filling(window_size, Density::ONE)?;
    let shown = showing(SURFACE, Density::ONE, laid_out(SURFACE, scale, 800, 480));
    assert_eq!(absent.sent()?, shown);
    assert_eq!(frame_size(&absent)?, Some(window_size));

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    requested.set_layout_size(Some(car_layout_size()?))?;
    assert_eq!(requested.sent()?, []);
    requested.created(window(&DisplaySurface::new(1280, 480)))?;
    assert_eq!(requested.sent()?, shown);
    Ok(())
}

#[test]
fn a_touch_on_a_layout_size_reaches_the_dp_point_shown_under_it() -> Result<(), TestError> {
    let mut host = shown_in_car_layout(&DisplaySurface::new(1280, 480))?;
    host.motion(MotionEvent::touch(
        0,
        TouchAction::Down(finger(0, 793.0, 216.0)),
    ))?;
    assert_eq!(host.sent()?, [touch_down(touched(495.625, 216.0))]);
    Ok(())
}

#[test]
fn a_window_resize_derives_a_new_scale_for_the_layout_size() -> Result<(), TestError> {
    let surface = DisplaySurface::new(1280, 480);
    let mut host = shown_in_car_layout(&surface)?;
    surface.resize(1600, 480);
    host.changed()?;
    let window_size = PixelSize::new(1600, 480);
    let scale = car_display_filling(window_size, Density::ONE)?;
    assert_eq!(host.sent()?, laid_out(SURFACE, scale, 800, 480));
    assert_eq!(frame_size(&host)?, Some(window_size));
    Ok(())
}

#[test]
fn a_density_change_lays_the_layout_size_out_at_the_new_density() -> Result<(), TestError> {
    let mut host = shown_in_car_layout(&DisplaySurface::new(1280, 480))?;
    host.display_changed(display(two_pixels_per_dp()?))?;
    host.motion(MotionEvent::touch(
        0,
        TouchAction::Down(finger(0, 793.0, 216.0)),
    ))?;
    let window_size = PixelSize::new(1280, 480);
    let mut reported = Vec::from(updated(SIDE / 2));
    reported.push(Sent::Surface(
        SURFACE,
        SurfaceEvent::DensityChanged(two_pixels_per_dp()?),
    ));
    let scale = car_display_filling(window_size, two_pixels_per_dp()?)?;
    reported.extend(laid_out(SURFACE, scale, 800, 480));
    reported.push(touch_down(touched(495.625, 216.0)));
    assert_eq!(host.sent()?, reported);
    assert_eq!(frame_size(&host)?, Some(window_size));
    Ok(())
}

#[test]
fn touches_on_a_window_without_pixels_use_the_last_layout() -> Result<(), TestError> {
    let surface = DisplaySurface::new(1280, 480);
    let mut host = shown_in_car_layout(&surface)?;
    host.motion(MotionEvent::touch(
        0,
        TouchAction::Down(finger(0, 793.0, 216.0)),
    ))?;
    surface.resize(0, 0);
    host.changed()?;
    host.motion(MotionEvent::touch(
        0,
        TouchAction::Up(finger(0, 793.0, 216.0)),
    ))?;
    let up = Sent::Device(DeviceEvent::TouchUp(touched(495.625, 216.0)));
    assert_eq!(host.sent()?, [touch_down(touched(495.625, 216.0)), up]);
    surface.resize(1600, 480);
    host.changed()?;
    let scale = car_display_filling(PixelSize::new(1600, 480), Density::ONE)?;
    assert_eq!(host.sent()?, laid_out(SURFACE, scale, 800, 480));
    Ok(())
}

#[test]
fn a_host_surface_without_pixels_is_laid_out_once_it_has_some() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    let surface = DisplaySurface::new(0, 0);
    host.created(window(&surface))?;
    host.changed()?;
    host.execute(create(SURFACE))?;
    assert_eq!(host.sent()?, showing(SURFACE, Density::ONE, []));
    host.motion(MotionEvent::touch(
        0,
        TouchAction::Down(finger(0, 0.0, 0.0)),
    ))?;
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
    let square = laid_out(SURFACE, PixelScale::SQUARE, SIDE, SIDE);
    assert_eq!(host.sent()?, showing(SURFACE, Density::ONE, square));
    surface.resize(2 * SIDE, SIDE);
    host.changed()?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, 2 * SIDE, SIDE);
    assert_eq!(host.sent()?, square);
    Ok(())
}

#[test]
fn a_cleared_layout_size_lays_the_surface_out_in_the_whole_window() -> Result<(), TestError> {
    let mut host = shown_in_car_layout(&DisplaySurface::new(1280, 480))?;
    host.set_layout_size(None)?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, 1280, 480);
    assert_eq!(host.sent()?, square);
    assert_eq!(frame_size(&host)?, Some(PixelSize::new(1280, 480)));
    Ok(())
}

#[test]
fn a_layout_size_too_long_at_the_density_fails_the_platform() -> Result<(), TestError> {
    let mut host = shown()?;
    host.display_changed(display(two_pixels_per_dp()?))?;
    let too_long = LayoutSize::try_new(Size::new(f64::MAX.dp(), 480.dp()))?;
    let set = host.set_layout_size(Some(too_long));
    assert!(matches!(set, Err(AospError::FitLayout(_))));
    Ok(())
}
