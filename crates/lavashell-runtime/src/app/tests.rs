// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use lavashell_core::geometry::Size;
use lavashell_core::units::{Dp, DpExt, NumberError};

fn app() -> App {
    App::new().facts(lavashell_test_fixtures::facts())
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
    assert_eq!(app.setup.surfaces.len(), 1);
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
    let mut app = app()
        .damage(DamageMode::Whole)
        .frame_timing(FrameTiming::Measured);
    app.layer(LayerSurface::new("bar"), |_| {});
    app.window(window("window"), |_| {})?;
    let setup = app.into_setup();
    assert!(matches!(
        setup.surfaces.as_slice(),
        [SurfaceSetup::Panel(..), SurfaceSetup::Window(..)]
    ));
    assert_eq!(setup.renderer.damage, DamageMode::Whole);
    assert_eq!(setup.renderer.frame_timing, FrameTiming::Measured);
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

fn surface_facts(app: &App) -> Vec<PlatformFacts> {
    (app.setup.surfaces.iter())
        .map(|surface| surface.view().platform())
        .collect()
}

#[test]
fn an_app_starts_from_the_default_platform_facts() -> TestResult {
    let mut app = App::new();
    app.window(window("window"), |_| {})?;
    assert_eq!(surface_facts(&app), [PlatformFacts::default()]);
    Ok(())
}

#[test]
fn facts_given_to_the_app_reach_the_surfaces_added_before_and_after() -> TestResult {
    let mut app = App::new();
    app.window(window("before"), |_| {})?;
    let mut app = app.facts(lavashell_test_fixtures::facts());
    app.layer(LayerSurface::new("after"), |_| {});
    assert_eq!(
        surface_facts(&app),
        [
            lavashell_test_fixtures::facts(),
            lavashell_test_fixtures::facts()
        ]
    );
    Ok(())
}
