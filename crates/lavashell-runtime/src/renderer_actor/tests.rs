// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_host::{NoControl, renderer_setup};
use lavashell_actor::{SystemClock, mailbox};
use lavashell_core::geometry::Size;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceState;
use lavashell_core::units::{Density, DpExt};
use lavashell_renderer::text::{Hinting, TextAntialiasing, TextDestination};
use lavashell_renderer::{HeadlessPresentation, Painter, PainterSetup};
use std::convert::Infallible;
use std::sync::mpsc;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the renderer could not start")]
    Start(#[from] SpawnError<RendererError<Infallible>>),
    #[error("the renderer failed")]
    Join(#[from] JoinError<RendererError<Infallible>>),
    #[error("the renderer stopped")]
    Stopped(#[from] RendererStopped),
    #[error("the mailbox is gone")]
    Closed(#[from] Closed),
    #[error("the output channel closed before the renderer sent an output")]
    Output(#[from] mpsc::RecvError),
}

type TestResult = Result<(), TestError>;

/// A started renderer with the setup of its painter, and the outputs of the renderer.
fn start(
    painter: Option<PainterSetup>,
) -> Result<(Renderer<NoControl>, mpsc::Receiver<RendererOutput>), TestError> {
    let (outputs, received) = mpsc::channel();
    let observer = move |output: RendererOutput| outputs.send(output).map_err(|_unsent| Closed);
    let setup = renderer_setup(painter);
    let presentation = HeadlessPresentation::without_surfaces();
    let clock = Arc::new(SystemClock::start());
    let renderer = Renderer::start(presentation, setup, clock, false, Box::new(observer))?;
    Ok((renderer, received))
}

fn empty_scene() -> Arc<BuiltSurface> {
    Arc::new(BuiltSurface {
        sequence: None,
        scene: SceneTree::default().snapshot(),
        state: SurfaceState::default(),
    })
}

fn render(surface: u64, sequence: u64) -> RendererInput<NoControl> {
    RendererInput::Render {
        surface: SurfaceId(surface),
        time: FrameTime {
            sequence,
            ..FrameTime::default()
        },
        viewport: Viewport::new(Size::new(4.dp(), 4.dp()), Density::ONE),
        built: empty_scene(),
    }
}

fn outline(surface: u64) -> RendererInput<NoControl> {
    RendererInput::Outline {
        surface: SurfaceId(surface),
        outline: None,
    }
}

fn control(surface: u64) -> RendererInput<NoControl> {
    RendererInput::Surface(SurfaceAttachment::Control {
        surface: SurfaceId(surface),
        message: (),
    })
}

fn detach(surface: u64) -> RendererInput<NoControl> {
    RendererInput::Surface(SurfaceAttachment::Detach(SurfaceId(surface)))
}

/// What the renderer takes from its mailbox, without the values that the merge rule ignores.
#[derive(Debug, PartialEq, Eq)]
enum Taken {
    Render(u64, u64),
    Outline(u64),
    Control(u64),
    CommitState(u64),
    AdvanceScene,
    Detach(u64),
    Attach,
    Stop,
}

impl Taken {
    fn of(input: &RendererInput<NoControl>) -> Self {
        match input {
            RendererInput::Render { surface, time, .. } => Self::Render(surface.0, time.sequence),
            RendererInput::Outline { surface, .. } => Self::Outline(surface.0),
            RendererInput::Surface(SurfaceAttachment::Control { surface, .. }) => {
                Self::Control(surface.0)
            }
            RendererInput::CommitState { surface, .. } => Self::CommitState(surface.0),
            RendererInput::AdvanceScene(_) => Self::AdvanceScene,
            RendererInput::Surface(SurfaceAttachment::Detach(surface)) => Self::Detach(surface.0),
            RendererInput::Surface(SurfaceAttachment::Attach(_)) => Self::Attach,
            RendererInput::Stop => Self::Stop,
        }
    }
}

/// The inputs that the renderer takes at once after `inputs` were pushed while it was busy.
fn taken(inputs: impl IntoIterator<Item = RendererInput<NoControl>>) -> Result<Vec<Taken>, Closed> {
    let (sender, receiver) = mailbox();
    for input in inputs {
        sender.push(input)?;
    }
    Ok(receiver.take()?.iter().map(Taken::of).collect())
}

#[test]
fn a_later_frame_or_outline_of_a_surface_replaces_one_not_handled_yet() -> TestResult {
    let inputs = [
        render(1, 1),
        outline(1),
        render(2, 1),
        render(1, 2),
        outline(1),
    ];
    assert_eq!(
        taken(inputs)?,
        [Taken::Render(1, 2), Taken::Outline(1), Taken::Render(2, 1)]
    );
    Ok(())
}

#[test]
fn a_message_a_commit_or_a_detach_keeps_the_frames_on_either_side() -> TestResult {
    let commit = RendererInput::CommitState {
        surface: SurfaceId(1),
        built: empty_scene(),
    };
    let inputs = [
        render(1, 1),
        control(1),
        render(1, 2),
        commit,
        render(1, 3),
        detach(1),
        render(1, 4),
    ];
    let expected = [
        Taken::Render(1, 1),
        Taken::Control(1),
        Taken::Render(1, 2),
        Taken::CommitState(1),
        Taken::Render(1, 3),
        Taken::Detach(1),
        Taken::Render(1, 4),
    ];
    assert_eq!(taken(inputs)?, expected);
    Ok(())
}

#[test]
fn a_later_scene_advance_replaces_one_not_handled_yet() -> TestResult {
    let viewport = Viewport::new(Size::new(4.dp(), 4.dp()), Density::ONE);
    let inputs = [
        RendererInput::AdvanceScene(viewport),
        render(1, 1),
        RendererInput::AdvanceScene(viewport),
    ];
    assert_eq!(taken(inputs)?, [Taken::AdvanceScene, Taken::Render(1, 1)]);
    Ok(())
}

#[test]
fn the_scene_deadline_is_reported_only_when_an_input_changes_it() {
    let passed = Duration::from_secs(1);
    let later = passed + Duration::from_millis(16);
    assert!(scene_wake(Some(passed), Some(passed)).is_none());
    assert!(scene_wake(None, None).is_none());
    assert!(matches!(
        scene_wake(Some(passed), Some(later)),
        Some(RendererOutput::SceneWake(Some(wake))) if wake == later
    ));
    assert!(matches!(
        scene_wake(Some(passed), None),
        Some(RendererOutput::SceneWake(None))
    ));
}

#[test]
fn the_renderer_reports_the_text_rendering_set_up_for_its_painter() -> TestResult {
    let rendering = TextRendering {
        hinting: Hinting::None,
        fractional_positioning: true,
        antialiasing: TextAntialiasing::Aliased,
    };
    let (mut renderer, _outputs) = start(Some(Box::new(move |painter: &mut Painter| {
        painter.set_text_rendering(rendering, TextDestination::default());
    })))?;
    assert_eq!(renderer.text_rendering(), rendering);
    renderer.stop()?;
    Ok(())
}

#[test]
fn a_detached_surface_is_reported_after_the_inputs_sent_before_it() -> TestResult {
    let (mut renderer, outputs) = start(None)?;
    renderer.send(control(3))?;
    renderer.send(detach(3))?;
    assert!(matches!(
        outputs.recv()?,
        RendererOutput::Retired(SurfaceId(3))
    ));
    renderer.stop()?;
    Ok(())
}

#[test]
fn a_failed_commit_ends_the_renderer_and_stop_returns_the_failure() -> TestResult {
    let (mut renderer, outputs) = start(None)?;
    let commit = RendererInput::CommitState {
        surface: SurfaceId(2),
        built: empty_scene(),
    };
    renderer.send(commit)?;
    assert!(matches!(
        renderer.stop(),
        Err(JoinError::Failed(RendererError::CommitState(_)))
    ));
    assert!(outputs.recv().is_err());
    Ok(())
}

#[test]
fn a_stopped_renderer_handles_no_more_inputs() -> TestResult {
    let (mut renderer, outputs) = start(None)?;
    renderer.stop()?;
    assert_eq!(renderer.send(detach(0)).err(), Some(RendererStopped));
    assert!(outputs.recv().is_err());
    Ok(())
}

#[test]
fn a_renderer_stops_while_a_link_from_a_platform_exists() -> TestResult {
    let (mut renderer, _outputs) = start(None)?;
    let link = renderer.attachments()?;
    renderer.stop()?;
    let detach = link.push(SurfaceAttachment::Detach(SurfaceId(0)));
    assert_eq!(detach, Err(Closed));
    Ok(())
}
