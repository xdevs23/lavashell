// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::{Color, Size, Viewport};
use lavashell_core::prelude::{Modifier, TextStyle};
use lavashell_core::reactive::State;
use lavashell_core::surface::ConfigureToken;
use lavashell_core::units::{Density, Dp, DpExt};
use lavashell_core::view::{FrameError, TextMeasurer, View};
use std::time::Duration;

const SURFACE: SurfaceId = SurfaceId(0);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("a view failed to compose a frame")]
    Frame(#[from] FrameError<Infallible>),
    #[error("a frame does not fit the scene in the builder")]
    Apply(#[from] ApplyError),
}

type TestResult = Result<(), TestError>;

struct NoText;

impl TextMeasurer for NoText {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::ZERO
    }
}

/// A view of one square filled with `color`.
fn square(color: &State<Color>) -> View {
    let read = color.clone();
    View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.spacer(20.dp(), 20.dp(), Modifier::new().background(read.get()));
    })
}

fn frame(view: &mut View, choreographers: &mut Choreographers) -> TestResult {
    let viewport = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    let failures = view.frame_into(viewport, &mut NoText, choreographers.sink(SURFACE))?;
    assert_eq!(failures.into_iter().count(), 0);
    Ok(())
}

fn leaves(builder: &SceneBuilder) -> usize {
    let snapshot = builder.snapshot();
    snapshot.at(Duration::ZERO).paint_order().leaves().count()
}

#[test]
fn a_frame_sent_to_the_sink_of_a_surface_shows_in_the_builder_once_it_is_built() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let mut state = SurfaceState::default();
    let mut view = square(&State::new(Color::WHITE));
    frame(&mut view, &mut choreographers)?;
    assert!(builder.is_empty());
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert_ne!(leaves(&builder), 0);
    Ok(())
}

#[test]
fn after_a_change_that_does_not_fit_the_builder_the_view_sends_the_whole_scene() -> TestResult {
    let mut choreographers = Choreographers::default();
    let color = State::new(Color::WHITE);
    let mut view = square(&color);
    let mut first = SceneBuilder::default();
    let mut state = SurfaceState::default();
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut first, &mut state)?;
    let mut builder = SceneBuilder::default();
    color.set(Color::TRANSPARENT);
    frame(&mut view, &mut choreographers)?;
    assert!(
        choreographers
            .build(SURFACE, &mut builder, &mut state)
            .is_err()
    );
    color.set(Color::WHITE);
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert_eq!(leaves(&builder), leaves(&first));
    Ok(())
}

#[test]
fn a_restart_replaces_the_scene_in_the_builder_instead_of_adding_to_it() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let mut state = SurfaceState::default();
    let color = State::new(Color::WHITE);
    let mut view = square(&color);
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    let shown = leaves(&builder);
    choreographers.retire(SURFACE);
    color.set(Color::oklch(0.0, 0.0, 0.0));
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert_eq!(leaves(&builder), shown);
    Ok(())
}

#[test]
fn a_retired_surface_builds_nothing() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let mut state = SurfaceState::default();
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.retire(SURFACE);
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert!(builder.is_empty());
    Ok(())
}

#[test]
fn a_frame_of_surface_state_changes_the_state_and_keeps_the_scene() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let mut state = SurfaceState::default();
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    let shown = leaves(&builder);
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    choreographers.sink(SURFACE).set_state(configure.clone());
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert!(!state.contains(&configure));
    choreographers.sink(SURFACE).end_state_frame();
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert!(state.contains(&configure));
    assert_eq!(leaves(&builder), shown);
    Ok(())
}

#[test]
fn after_a_restart_the_state_waits_for_the_whole_scene_and_the_builder_keeps_its_scene()
-> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let mut state = SurfaceState::default();
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    let shown = leaves(&builder);
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    let sink = choreographers.sink(SURFACE);
    let Ok(()) = sink.restart();
    sink.set_state(configure.clone());
    sink.end_state_frame();
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert!(!state.contains(&configure));
    assert_eq!(leaves(&builder), shown);
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder, &mut state)?;
    assert!(state.contains(&configure));
    assert_eq!(leaves(&builder), shown);
    Ok(())
}

#[test]
fn a_restart_sends_the_surface_state_again_with_the_whole_scene() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut state = SurfaceState::default();
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    let sink = choreographers.sink(SURFACE);
    sink.set_state(configure.clone());
    sink.end_state_frame();
    let Ok(()) = sink.restart();
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.build(SURFACE, &mut SceneBuilder::default(), &mut state)?;
    assert!(state.contains(&configure));
    Ok(())
}

#[test]
fn a_change_of_the_surface_state_applies_when_the_scene_in_its_frame_does_not_fit() -> TestResult {
    let mut choreographers = Choreographers::default();
    let color = State::new(Color::WHITE);
    let mut view = square(&color);
    let mut state = SurfaceState::default();
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut SceneBuilder::default(), &mut state)?;
    color.set(Color::TRANSPARENT);
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    choreographers.sink(SURFACE).set_state(configure.clone());
    frame(&mut view, &mut choreographers)?;
    let built = choreographers.build(SURFACE, &mut SceneBuilder::default(), &mut state);
    assert!(built.is_err());
    assert!(state.contains(&configure));
    Ok(())
}
