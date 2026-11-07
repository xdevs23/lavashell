// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::JoinError;
use lavashell_core::geometry::{Color, Size, Viewport};
use lavashell_core::prelude::{Modifier, TextStyle};
use lavashell_core::reactive::State;
use lavashell_core::scene::tree::SceneNodeId;
use lavashell_core::surface::ConfigureToken;
use lavashell_core::units::{Density, Dp, DpExt};
use lavashell_core::view::{FrameError, TextMeasurer, View};
use std::sync::mpsc;
use std::time::Duration;

const SURFACE: SurfaceId = SurfaceId(0);

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the scene pipeline failed")]
    Pipeline(#[from] Error),
    #[error("a view failed to send a frame")]
    Frame(#[from] FrameError<Closed>),
    #[error("the choreographer is gone")]
    Closed(#[from] Closed),
    #[error("the scene builder sent no report")]
    Report(#[from] mpsc::RecvError),
}

type TestResult = Result<(), TestError>;

struct NoText;

impl TextMeasurer for NoText {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::ZERO
    }
}

/// A started pipeline, and the reports of its scene builder.
fn start() -> Result<(ScenePipeline, mpsc::Receiver<BuilderReport>), Error> {
    let (reports, received) = mpsc::channel();
    let observer = move |report: BuilderReport| reports.send(report).map_err(|_unsent| Closed);
    Ok((ScenePipeline::start(Box::new(observer))?, received))
}

/// A view of one square filled with `color`.
fn square(color: &State<Color>) -> View {
    let read = color.clone();
    View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.spacer(20.dp(), 20.dp(), Modifier::new().background(read.get()));
    })
}

fn frame(view: &mut View, pipeline: &mut ScenePipeline, end: Option<FrameSequence>) -> TestResult {
    let viewport = Viewport::new(Size::new(100.dp(), 100.dp()), Density::ONE);
    let failures = view.frame_into(viewport, &mut NoText, &mut pipeline.sink(SURFACE, end))?;
    assert_eq!(failures.into_iter().count(), 0);
    Ok(())
}

/// Sends a frame that removes a node that no scene has.
fn send_bad_frame(pipeline: &mut ScenePipeline) -> Result<(), Closed> {
    let mut sink = pipeline.sink(SURFACE, None);
    sink.change(Change::Remove {
        id: SceneNodeId(u64::MAX),
    })?;
    sink.end_frame()
}

fn leaves(snapshot: &SceneSnapshot) -> usize {
    snapshot.at(Duration::ZERO).paint_order().leaves().count()
}

#[test]
fn a_frame_shows_in_the_scene_of_its_surface_once_the_builder_reports_it_built() -> TestResult {
    let (mut pipeline, reports) = start()?;
    let mut view = square(&State::new(Color::WHITE));
    frame(&mut view, &mut pipeline, Some(FrameSequence(7)))?;
    let report = reports.recv()?;
    assert!(matches!(
        report,
        BuilderReport::Built {
            surface: SURFACE,
            sequence: Some(FrameSequence(7)),
            change: BuiltChange::Scene,
        }
    ));
    assert_ne!(leaves(&pipeline.built(SURFACE).snapshot), 0);
    pipeline.stop()?;
    Ok(())
}

#[test]
fn a_stale_scene_skips_each_frame_until_the_view_sends_the_whole_scene_to_the_restarted_sink()
-> TestResult {
    let (mut pipeline, reports) = start()?;
    let mut view = square(&State::new(Color::WHITE));
    frame(&mut view, &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let shown = pipeline.built(SURFACE);
    send_bad_frame(&mut pipeline)?;
    let restart = reports.recv()?;
    assert!(matches!(
        restart,
        BuilderReport::Restart {
            surface: SURFACE,
            ..
        }
    ));
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    pipeline.sink(SURFACE, None).end_frame()?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    assert!(Arc::ptr_eq(&pipeline.built(SURFACE), &shown));
    pipeline.restart(SURFACE);
    view.invalidate();
    frame(&mut view, &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let resent = pipeline.built(SURFACE);
    assert_ne!(resent.snapshot.lineage(), shown.snapshot.lineage());
    assert_eq!(leaves(&resent.snapshot), leaves(&shown.snapshot));
    pipeline.stop()?;
    Ok(())
}

#[test]
fn a_first_frame_that_does_not_fit_stops_the_scene_builder_with_the_failure() -> TestResult {
    let (mut pipeline, _reports) = start()?;
    send_bad_frame(&mut pipeline)?;
    let stopped = pipeline.stop();
    assert!(matches!(
        stopped,
        Err(Error::RunBuilder(JoinError::Failed(_)))
    ));
    let change = Change::Remove { id: SceneNodeId(1) };
    assert_eq!(pipeline.sink(SURFACE, None).change(change), Err(Closed));
    Ok(())
}

#[test]
fn a_retired_surface_has_an_empty_scene() -> TestResult {
    let (mut pipeline, reports) = start()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    pipeline.retire(SURFACE)?;
    pipeline.stop()?;
    assert!(pipeline.built(SURFACE).snapshot.is_empty());
    Ok(())
}

#[test]
fn a_frame_without_changes_reports_its_sequence_and_publishes_nothing() -> TestResult {
    let (mut pipeline, reports) = start()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let shown = pipeline.built(SURFACE);
    assert!(!pipeline.sink(SURFACE, None).end_state_frame()?);
    assert!(
        pipeline
            .sink(SURFACE, Some(FrameSequence(3)))
            .end_state_frame()?
    );
    let report = reports.recv()?;
    assert!(matches!(
        report,
        BuilderReport::Built {
            surface: SURFACE,
            sequence: Some(FrameSequence(3)),
            change: BuiltChange::Nothing,
        }
    ));
    assert!(Arc::ptr_eq(&pipeline.built(SURFACE), &shown));
    pipeline.stop()?;
    Ok(())
}

#[test]
fn a_frame_of_surface_state_changes_the_state_and_keeps_the_scene() -> TestResult {
    let (mut pipeline, reports) = start()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let shown = pipeline.built(SURFACE);
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    pipeline.sink(SURFACE, None).set_state(configure.clone())?;
    assert!(!pipeline.built(SURFACE).state.contains(&configure));
    assert!(pipeline.sink(SURFACE, None).end_state_frame()?);
    let report = reports.recv()?;
    assert!(matches!(
        report,
        BuilderReport::Built {
            change: BuiltChange::State,
            ..
        }
    ));
    let built = pipeline.built(SURFACE);
    assert!(built.state.contains(&configure));
    assert_eq!(built.snapshot.lineage(), shown.snapshot.lineage());
    assert_eq!(leaves(&built.snapshot), leaves(&shown.snapshot));
    pipeline.stop()?;
    Ok(())
}

#[test]
fn after_a_restart_the_state_waits_for_the_whole_scene_and_the_builder_keeps_its_scene()
-> TestResult {
    let (mut pipeline, reports) = start()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let shown = pipeline.built(SURFACE);
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    let mut sink = pipeline.sink(SURFACE, None);
    sink.restart()?;
    sink.set_state(configure.clone())?;
    assert!(!sink.end_state_frame()?);
    assert!(Arc::ptr_eq(&pipeline.built(SURFACE), &shown));
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let built = pipeline.built(SURFACE);
    assert!(built.state.contains(&configure));
    assert_eq!(leaves(&built.snapshot), leaves(&shown.snapshot));
    pipeline.stop()?;
    Ok(())
}

#[test]
fn a_restart_sends_the_surface_state_again_with_the_whole_scene() -> TestResult {
    let (mut pipeline, reports) = start()?;
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    let mut sink = pipeline.sink(SURFACE, None);
    sink.set_state(configure.clone())?;
    sink.restart()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    assert!(pipeline.built(SURFACE).state.contains(&configure));
    pipeline.stop()?;
    Ok(())
}

#[test]
fn a_change_of_the_surface_state_applies_when_the_scene_in_its_frame_does_not_fit() -> TestResult {
    let (mut pipeline, reports) = start()?;
    frame(&mut square(&State::new(Color::WHITE)), &mut pipeline, None)?;
    assert!(matches!(reports.recv()?, BuilderReport::Built { .. }));
    let configure = SurfaceChange::Configure(ConfigureToken::default());
    pipeline.sink(SURFACE, None).set_state(configure.clone())?;
    send_bad_frame(&mut pipeline)?;
    assert!(matches!(reports.recv()?, BuilderReport::Restart { .. }));
    let report = reports.recv()?;
    assert!(matches!(
        report,
        BuilderReport::Built {
            change: BuiltChange::State,
            ..
        }
    ));
    assert!(pipeline.built(SURFACE).state.contains(&configure));
    pipeline.stop()?;
    Ok(())
}
