// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::geometry::{Color, Size, Viewport};
use lavashell_core::prelude::{Modifier, TextStyle};
use lavashell_core::reactive::State;
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
    let mut view = square(&State::new(Color::WHITE));
    frame(&mut view, &mut choreographers)?;
    assert!(builder.is_empty());
    choreographers.build(SURFACE, &mut builder)?;
    assert_ne!(leaves(&builder), 0);
    Ok(())
}

#[test]
fn after_a_change_that_does_not_fit_the_builder_the_view_sends_the_whole_scene() -> TestResult {
    let mut choreographers = Choreographers::default();
    let color = State::new(Color::WHITE);
    let mut view = square(&color);
    let mut first = SceneBuilder::default();
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut first)?;
    let mut builder = SceneBuilder::default();
    color.set(Color::TRANSPARENT);
    frame(&mut view, &mut choreographers)?;
    assert!(choreographers.build(SURFACE, &mut builder).is_err());
    color.set(Color::WHITE);
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder)?;
    assert_eq!(leaves(&builder), leaves(&first));
    Ok(())
}

#[test]
fn a_restart_replaces_the_scene_in_the_builder_instead_of_adding_to_it() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    let color = State::new(Color::WHITE);
    let mut view = square(&color);
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder)?;
    let shown = leaves(&builder);
    choreographers.retire(SURFACE);
    color.set(Color::oklch(0.0, 0.0, 0.0));
    frame(&mut view, &mut choreographers)?;
    choreographers.build(SURFACE, &mut builder)?;
    assert_eq!(leaves(&builder), shown);
    Ok(())
}

#[test]
fn a_retired_surface_builds_nothing() -> TestResult {
    let mut choreographers = Choreographers::default();
    let mut builder = SceneBuilder::default();
    frame(&mut square(&State::new(Color::WHITE)), &mut choreographers)?;
    choreographers.retire(SURFACE);
    choreographers.build(SURFACE, &mut builder)?;
    assert!(builder.is_empty());
    Ok(())
}
