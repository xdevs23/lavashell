// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use lavashell_core::geometry::Size;
use lavashell_core::units::{Dp, DpExt, NumberError};

fn app() -> App {
    App::new(lavashell_test_fixtures::facts())
}

fn window(title: &str) -> Window {
    Window::new(title, "test.app")
}

#[test]
fn a_window_size_must_be_finite_and_above_zero() {
    let mut app = app();
    let flat = window("flat").size(100.dp(), Dp::ZERO);
    assert!(matches!(
        app.window(flat, |_| {}),
        Err(SetupError::WindowSize {
            size,
            source: NumberError::OutOfRange,
        }) if size == Size::new(100.dp(), Dp::ZERO)
    ));
    let endless = window("endless").size(Dp::INFINITY, 100.dp());
    assert!(matches!(
        app.window(endless, |_| {}),
        Err(SetupError::WindowSize {
            source: NumberError::NotFinite,
            ..
        })
    ));
    assert!(!app.setup.has_surfaces());
}

#[test]
fn a_minimum_window_size_must_be_finite_and_at_least_zero() {
    let mut app = app();
    let negative = window("negative").min_size(-1.dp(), 10.dp());
    assert!(matches!(
        app.window(negative, |_| {}),
        Err(SetupError::MinimumWindowSize {
            source: NumberError::Negative,
            ..
        })
    ));
    assert!(!app.setup.has_surfaces());
}

#[test]
fn a_window_below_one_dp_is_accepted() -> TestResult {
    let mut app = app();
    let tiny = window("tiny")
        .size(0.3.dp(), 0.3.dp())
        .min_size(Dp::ZERO, Dp::ZERO);
    app.window(tiny, |_| {})?;
    assert_eq!(app.setup.windows().count(), 1);
    Ok(())
}

#[test]
fn an_app_has_at_most_one_session_lock_and_one_per_output_layer_factory() -> TestResult {
    let mut app = app();
    app.session_lock(SessionLock::new(), |_| {})?;
    app.layers_per_output(|_| Vec::new())?;
    assert!(matches!(
        app.session_lock(SessionLock::new(), |_| {}),
        Err(SetupError::SessionLockConfigured)
    ));
    assert!(matches!(
        app.layers_per_output(|_| Vec::new()),
        Err(SetupError::OutputLayersConfigured)
    ));
    Ok(())
}

#[test]
fn the_setup_keeps_the_surfaces_in_order_and_the_renderer_settings() -> TestResult {
    let mut app = app().incremental(false).gpu_timing(true);
    app.layer(LayerSurface::new("bar"), |_| {});
    app.window(window("window"), |_| {})?;
    let setup = app.into_setup();
    assert!(matches!(
        setup.surfaces.as_slice(),
        [SurfaceSetup::Panel(..), SurfaceSetup::Window(..)]
    ));
    assert!(setup.renderer.full_redraw);
    assert!(setup.renderer.gpu_timing);
    Ok(())
}

#[test]
fn a_session_lock_alone_gives_the_setup_a_surface() -> TestResult {
    let mut app = app();
    assert!(!app.setup.has_surfaces());
    app.session_lock(SessionLock::new(), |_| {})?;
    assert!(app.into_setup().has_surfaces());
    Ok(())
}
