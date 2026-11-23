// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::{Error, HeadlessPresentation};
use lavashell_core::prelude::{
    Brush, Color, Density, Dp, DpExt, DrawCommand, FrameTime, PixelScale, Presented, Rect,
    ScaledViewport, SceneTree, Size, StreamId, StreamImage, Viewport,
};
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::{Change, Content, Node, Packet, SceneNodeId};
use lavashell_core::surface::SurfaceChange;
use lavashell_platform::{FrameOutcome, SurfaceAlpha};
use lavashell_renderer::test_scenes::insert;
use lavashell_renderer::test_surfaces::{TestSurface, WIDE_SURFACE};
use lavashell_renderer::text::{Hinting, TextAntialiasing, TextDestination};
use lavashell_renderer::{Fonts, FrameDevice, StreamFrames};
use lavashell_test_fixtures::show_all;
use lavashell_vulkan::HeadlessWindow;
use skia_safe::IPoint;
use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::Arc;

const SURFACE_1: SurfaceId = SurfaceId::numbered(1);

const SURFACE: SurfaceId = SurfaceId::numbered(0);

const WIDTH: u32 = 40;

const HEIGHT: u32 = 30;

pub(super) fn viewport() -> Viewport {
    let size = Size::new(Dp::new(f64::from(WIDTH)), Dp::new(f64::from(HEIGHT)));
    Viewport::new(size, Density::ONE)
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Prepare(Viewport),
    BeforePresent(Viewport),
    AfterPresent(FrameOutcome),
    ImportStreams,
    ReleaseStreams,
    CommitState(SurfaceState),
}

struct RecordingControl {
    events: Rc<RefCell<Vec<Event>>>,
}

impl SurfaceControl for RecordingControl {
    type Error = Infallible;
    type Message = Infallible;

    fn receive(&mut self, message: Infallible) {
        match message {}
    }

    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), Infallible> {
        self.events
            .borrow_mut()
            .push(Event::Prepare(frame.scaled_viewport.viewport));
        Ok(())
    }

    fn before_present(&mut self, frame: &PresentFrame) {
        self.events
            .borrow_mut()
            .push(Event::BeforePresent(frame.scaled_viewport.viewport));
    }

    fn after_present(&mut self, outcome: FrameOutcome) {
        self.events.borrow_mut().push(Event::AfterPresent(outcome));
    }

    fn commit_state(&mut self, state: &SurfaceState) -> Result<(), Infallible> {
        self.events
            .borrow_mut()
            .push(Event::CommitState(state.clone()));
        Ok(())
    }
}

struct FailingStreams {
    events: Rc<RefCell<Vec<Event>>>,
}

impl StreamBackend for FailingStreams {
    type Gpu = VulkanRenderer;
    type Error = Error;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        _: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Error>) {
        self.events.borrow_mut().push(Event::ImportStreams);
        let drawn = draw(FrameDevice {
            gpu,
            painter,
            streams: &StreamFrames::default(),
        });
        self.events.borrow_mut().push(Event::ReleaseStreams);
        (drawn, Err(Error::SkiaAbandoned))
    }
}

fn setup() -> RendererSetup {
    RendererSetup {
        fonts: Fonts::new(),
        painter: None,
        frame_timing: lavashell_renderer::FrameTiming::Off,
        damage: DamageMode::Tracked,
    }
}

fn state() -> Result<RendererState<RecordingControl>, TestError> {
    Ok(RendererState::new(
        &HeadlessPresentation::with_surfaces(),
        setup(),
    )?)
}

fn attach(events: &Rc<RefCell<Vec<Event>>>) -> Attach<RecordingControl, VulkanWindow> {
    Attach {
        surface: SURFACE,
        window: Arc::new(HeadlessWindow),
        alpha: SurfaceAlpha::Opaque,
        output: OutputRequest::default(),
        hdr_metadata: None,
        control: RecordingControl {
            events: Rc::clone(events),
        },
    }
}

fn attach_recording<S: RendererStreams>(
    state: &mut RendererState<RecordingControl, S>,
    events: &Rc<RefCell<Vec<Event>>>,
) -> Result<(), TestError> {
    state.attach(attach(events))?;
    Ok(())
}

fn frame(sequence: u64, state: &SurfaceState) -> PresentFrame<'_> {
    PresentFrame {
        time: FrameTime {
            sequence,
            ..FrameTime::default()
        },
        scaled_viewport: ScaledViewport {
            viewport: viewport(),
            scale: PixelScale::SQUARE,
        },
        present_at: None,
        state,
    }
}

pub(super) fn scene() -> Result<SceneSnapshot, TestError> {
    let fill = DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()).into(),
        brush: Brush::Solid(Color::WHITE),
    };
    Ok(SceneTree::with_leaf([fill])?.snapshot())
}

#[test]
fn each_shown_draw_prepares_requests_and_reports_its_present_in_order() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let snapshot = scene()?;
    let surface_state = SurfaceState::default();
    for sequence in [1, 2] {
        let frame = frame(sequence, &surface_state);
        assert_eq!(
            state.draw(SURFACE, frame, &snapshot)?.outcome,
            FrameOutcome::Shown
        );
        assert_eq!(
            events.take(),
            [
                Event::Prepare(frame.scaled_viewport.viewport),
                Event::BeforePresent(frame.scaled_viewport.viewport),
                Event::AfterPresent(FrameOutcome::Shown),
            ]
        );
    }
    Ok(())
}

#[test]
fn the_text_rendering_is_the_one_the_setup_of_the_painter_chose() -> Result<(), TestError> {
    let rendering = TextRendering {
        hinting: Hinting::None,
        fractional_positioning: true,
        antialiasing: TextAntialiasing::Aliased,
    };
    let setup = RendererSetup {
        painter: Some(Box::new(move |painter: &mut Painter| {
            painter.set_text_rendering(rendering, TextDestination::default());
        })),
        ..setup()
    };
    let state =
        RendererState::<RecordingControl>::new(&HeadlessPresentation::with_surfaces(), setup)?;
    assert_eq!(state.text_rendering(), rendering);
    Ok(())
}

#[test]
fn a_surface_attaches_once_and_draws_no_more_once_retired() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let twice = state.attach(attach(&events));
    assert!(matches!(twice, Err(StateError::AttachedTwice(SURFACE))));
    assert!(state.retire(SURFACE).is_some());
    let drawn = state.draw(SURFACE, frame(1, &SurfaceState::default()), &scene()?);
    assert!(matches!(
        drawn,
        Err(DrawError::Frame(FrameError::State(
            StateError::UnknownSurface(SURFACE)
        )))
    ));
    Ok(())
}

#[test]
fn streams_surround_the_present_and_fail_with_the_drawn_frame() -> Result<(), TestError> {
    let events = Rc::default();
    let streams = FailingStreams {
        events: Rc::clone(&events),
    };
    let presentation = HeadlessPresentation::with_surfaces();
    let mut state = RendererState::with_streams(&presentation, setup(), None, |_| Ok(streams))?;
    attach_recording(&mut state, &events)?;
    let surface_state = SurfaceState::default();
    let frame = frame(1, &surface_state);
    let drawn = state.draw(SURFACE, frame, &scene()?);
    assert!(matches!(
        drawn,
        Err(DrawError::StreamsOfShownFrame {
            drawn: Drawn {
                outcome: FrameOutcome::Shown,
                ..
            },
            streams: Error::SkiaAbandoned,
        })
    ));
    assert_eq!(
        events.take(),
        [
            Event::Prepare(frame.scaled_viewport.viewport),
            Event::ImportStreams,
            Event::BeforePresent(frame.scaled_viewport.viewport),
            Event::AfterPresent(FrameOutcome::Shown),
            Event::ReleaseStreams,
        ]
    );
    Ok(())
}

const STREAM_FRAME: TestSurface = TestSurface::new(16, 12);

const STREAM_AREA: Rect = Rect::new(Dp::new(8.0), Dp::new(6.0), Dp::new(16.0), Dp::new(12.0));

fn stream_packet(image: StreamImage) -> Result<Packet, UnbalancedRestore> {
    let stream = DrawCommand::StreamImage {
        image,
        destination: STREAM_AREA,
        frame: Rect::from_size(STREAM_AREA.size()),
    };
    Packet::new([stream].into())
}

fn painted_pixel<S>(
    state: &mut RendererState<RecordingControl, S>,
    x: u16,
    y: u16,
) -> Result<[u8; 4], TestError> {
    let attached = (state.surfaces.get_mut(&SURFACE)).ok_or(TestError::Missing("surface"))?;
    let pixels = (attached.target.painted_pixels()).ok_or(TestError::Missing("painted image"))?;
    Ok(pixels.at(x, y))
}

#[test]
fn a_rebound_stream_frame_repaints_its_area_and_leaves_the_rest() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let surface_state = SurfaceState::default();
    let stream = StreamId::next();
    let image = |generation| StreamImage {
        stream,
        generation,
        size: STREAM_FRAME.pixel_size(),
    };
    let background = DrawCommand::Fill {
        shape: Rect::from_size(viewport().size).into(),
        brush: Brush::Solid(Color::WHITE),
    };
    let mut tree = SceneTree::default();
    let changes = [
        insert(
            1,
            None,
            0,
            Node::Leaf(Content::Packet(Packet::new([background].into())?)),
        ),
        insert(
            2,
            None,
            1,
            Node::Leaf(Content::Packet(stream_packet(image(1))?)),
        ),
    ];
    show_all(&mut tree, changes)?;
    let mut red = state.gpu.offscreen(STREAM_FRAME.pixel_size())?;
    let _canvas_returned_for_chaining = red.canvas().clear(skia_safe::Color::RED);
    (state.streams.frames_mut()).bind(&image(1), red.image_snapshot().into(), IPoint::new(0, 0));
    let drawn = state.draw(SURFACE, frame(1, &surface_state), &tree.snapshot())?;
    assert_eq!(drawn.outcome, FrameOutcome::Shown);
    assert_eq!(painted_pixel(&mut state, 16, 12)?, [255, 0, 0, 255]);
    let outside = painted_pixel(&mut state, 2, 2)?;
    let mut blue = state.gpu.offscreen(STREAM_FRAME.pixel_size())?;
    let _canvas_returned_for_chaining = blue.canvas().clear(skia_safe::Color::BLUE);
    (state.streams.frames_mut()).bind(&image(2), blue.image_snapshot().into(), IPoint::new(0, 0));
    tree.apply_change(
        SceneTree::ROOT,
        Change::SetContent {
            id: SceneNodeId(2),
            content: Content::Packet(stream_packet(image(2))?),
        },
    )?;
    let drawn = state.draw(SURFACE, frame(2, &surface_state), &tree.snapshot())?;
    assert_eq!(drawn.outcome, FrameOutcome::Shown);
    assert_eq!(painted_pixel(&mut state, 16, 12)?, [0, 0, 255, 255]);
    assert_eq!(painted_pixel(&mut state, 2, 2)?, outside);
    state.streams.frames_mut().retire(stream);
    Ok(())
}

#[test]
fn a_committed_state_goes_to_the_control_of_its_surface_without_a_draw() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let mut committed = SurfaceState::default();
    committed.apply(SurfaceChange::InputRegion(Some(Vec::new())));
    state.commit_state(SURFACE, &committed)?;
    assert_eq!(events.take(), [Event::CommitState(committed.clone())]);
    let unknown = state.commit_state(SurfaceId::numbered(1), &committed);
    assert!(matches!(
        unknown,
        Err(CommitError::State(StateError::UnknownSurface(SURFACE_1)))
    ));
    Ok(())
}

#[test]
fn a_frame_at_a_non_square_scale_fills_exactly_the_scaled_buffer() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let surface_state = SurfaceState::default();
    let square = frame(1, &surface_state);
    let wide = ScaledViewport {
        scale: PixelScale::filling(square.scaled_viewport.viewport, WIDE_SURFACE.pixel_size())?,
        ..square.scaled_viewport
    };
    let background = DrawCommand::Fill {
        shape: Rect::from_size(wide.viewport.size).into(),
        brush: Brush::Solid(Color::WHITE),
    };
    let snapshot = SceneTree::with_leaf([background])?.snapshot();
    let wide_frame = PresentFrame {
        scaled_viewport: wide,
        ..square
    };
    assert_eq!(
        state.draw(SURFACE, wide_frame, &snapshot)?.outcome,
        FrameOutcome::Shown
    );
    let attached = (state.surfaces.get_mut(&SURFACE)).ok_or(TestError::Missing("surface"))?;
    let pixels = (attached.target.painted_pixels()).ok_or(TestError::Missing("painted image"))?;
    assert_eq!(pixels.rows()?.map(<[_]>::len).collect::<Vec<_>>(), [64; 30]);
    assert!(pixels.iter().all(|&pixel| pixel == [255; 4]));
    assert_eq!(state.contents(SURFACE, wide)?, SurfaceContents::Current);
    assert_eq!(
        state.contents(
            SURFACE,
            ScaledViewport {
                scale: PixelScale::SQUARE,
                ..wide
            }
        )?,
        SurfaceContents::Outdated
    );
    Ok(())
}
