// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_host::{NoControl, renderer_setup};
use lavashell_actor::{SystemClock, mailbox};
use lavashell_core::geometry::{PixelSize, Size};
use lavashell_core::output::OutputRequest;
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::SurfaceState;
use lavashell_core::units::{Density, DpExt, NumberError};
use lavashell_platform::no_streams;
use lavashell_renderer::text::{Hinting, TextAntialiasing, TextDestination};
use lavashell_renderer::{
    Attach, HeadlessPresentation, NoStreams, Painter, PainterSetup, PresentationPolicy,
    SurfaceAlpha, SurfaceTiming,
};
use lavashell_vulkan::HeadlessWindow;
use std::sync::mpsc;

type TestInput = RendererInput<NoControl, NoStreams>;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the renderer could not start")]
    Start(#[from] SpawnError<RendererError<NoControl, NoStreams>>),
    #[error("the renderer failed")]
    Join(#[from] JoinError<RendererError<NoControl, NoStreams>>),
    #[error("the renderer stopped")]
    Stopped(#[from] RendererStopped),
    #[error("the mailbox is gone")]
    Closed(#[from] Closed),
    #[error("the output channel closed before the renderer sent an output")]
    Output(#[from] mpsc::RecvError),
    #[error("a number of the test is out of range")]
    Number(#[from] NumberError),
}

type TestResult = Result<(), TestError>;

/// A started renderer with the setup of its painter, and the outputs of the renderer.
fn start(
    painter: Option<PainterSetup>,
) -> Result<
    (
        Renderer<NoControl, NoStreams>,
        mpsc::Receiver<RendererOutput>,
    ),
    TestError,
> {
    let (outputs, received) = mpsc::channel();
    let observer = move |output: RendererOutput| outputs.send(output).map_err(|_unsent| Closed);
    let setup = renderer_setup(painter);
    let presentation = HeadlessPresentation::with_surfaces();
    let clock = Arc::new(SystemClock::start());
    let observer = Box::new(observer);
    let renderer = Renderer::start(presentation, setup, no_streams(), clock, false, observer)?;
    Ok((renderer, received))
}

fn empty_scene() -> Arc<BuiltSurface> {
    Arc::new(BuiltSurface {
        sequence: None,
        scene: SceneTree::default().snapshot(),
        state: SurfaceState::default(),
    })
}

fn viewport() -> Viewport {
    Viewport::new(Size::new(4.dp(), 4.dp()), Density::ONE)
}

/// The frame `sequence` of `SurfaceId(surface)` in `viewport()` at `scale`.
fn render_at(surface: u64, sequence: u64, scale: PixelScale) -> TestInput {
    RendererInput::Render {
        surface: SurfaceId(surface),
        time: FrameTime {
            sequence,
            ..FrameTime::default()
        },
        viewport: viewport(),
        scale,
        built: empty_scene(),
    }
}

fn render(surface: u64, sequence: u64) -> TestInput {
    render_at(surface, sequence, PixelScale::SQUARE)
}

/// The attachment of `SurfaceId(surface)` to a window that presents nowhere.
fn attach(surface: u64) -> TestInput {
    RendererInput::Surface(SurfaceAttachment::Attach(Box::new(Attach {
        surface: SurfaceId(surface),
        window: Arc::new(HeadlessWindow),
        alpha: SurfaceAlpha::Opaque,
        timing: SurfaceTiming::new(PresentationPolicy::Automatic),
        output: OutputRequest::default(),
        hdr_metadata: None,
        gpu_budget: None,
        control: NoControl,
    })))
}

fn outline(surface: u64) -> TestInput {
    RendererInput::Outline {
        surface: SurfaceId(surface),
        outline: None,
    }
}

fn control(surface: u64) -> TestInput {
    RendererInput::Surface(SurfaceAttachment::Control {
        surface: SurfaceId(surface),
        message: (),
    })
}

fn detach(surface: u64) -> TestInput {
    RendererInput::Surface(SurfaceAttachment::Detach(SurfaceId(surface)))
}

fn stream_task(task: impl FnOnce(&mut NoStreams, &Painter) + Send + 'static) -> TestInput {
    RendererInput::Streams(Box::new(task))
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
    Streams,
    Stop,
}

impl Taken {
    fn of(input: &TestInput) -> Self {
        match input {
            RendererInput::Render { surface, time, .. } => Self::Render(surface.0, time.sequence),
            RendererInput::Outline { surface, .. } => Self::Outline(surface.0),
            RendererInput::Surface(SurfaceAttachment::Control { surface, .. }) => {
                Self::Control(surface.0)
            }
            RendererInput::CommitState { surface, .. } => Self::CommitState(surface.0),
            RendererInput::AdvanceScene { .. } => Self::AdvanceScene,
            RendererInput::Surface(SurfaceAttachment::Detach(surface)) => Self::Detach(surface.0),
            RendererInput::Surface(SurfaceAttachment::Attach(_)) => Self::Attach,
            RendererInput::Streams(_) => Self::Streams,
            RendererInput::Stop => Self::Stop,
        }
    }
}

/// The inputs that the renderer takes at once after `inputs` were pushed while it was busy.
fn taken(inputs: impl IntoIterator<Item = TestInput>) -> Result<Vec<Taken>, Closed> {
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
fn a_stream_task_keeps_the_frames_on_either_side() -> TestResult {
    let inputs = [render(1, 1), stream_task(|_, _| {}), render(1, 2)];
    let expected = [Taken::Render(1, 1), Taken::Streams, Taken::Render(1, 2)];
    assert_eq!(taken(inputs)?, expected);
    Ok(())
}

#[test]
fn a_later_scene_advance_replaces_one_not_handled_yet() -> TestResult {
    let advance = || RendererInput::AdvanceScene {
        viewport: viewport(),
        scale: PixelScale::SQUARE,
    };
    let inputs = [advance(), render(1, 1), advance()];
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
fn each_frame_is_drawn_at_the_scale_that_it_carries() -> TestResult {
    let (mut renderer, outputs) = start(None)?;
    renderer.send(attach(1))?;
    let wide = PixelScale::filling(viewport(), PixelSize::new(8, 4))?;
    for (sequence, scale) in [(1, wide), (2, PixelScale::SQUARE)] {
        renderer.send(render_at(1, sequence, scale))?;
        let drawn = match outputs.recv()? {
            RendererOutput::Rendered(frame) => Some((frame.scale, frame.contents)),
            _ => None,
        };
        assert_eq!(drawn, Some((scale, SurfaceContents::Current)));
    }
    renderer.stop()?;
    Ok(())
}

#[test]
fn a_stream_task_runs_on_the_renderer_with_its_streams() -> TestResult {
    let (mut renderer, _outputs) = start(None)?;
    let (ran, received) = mpsc::channel();
    renderer.send(stream_task(move |_, _| {
        let _test_waits_below = ran.send(());
    }))?;
    received.recv()?;
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
