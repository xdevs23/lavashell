// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::stream::FrameDevice;
use crate::test_error::TestError;
use crate::test_scenes::insert;
use crate::{Error, PresentationPolicy, StreamFrames};
use ash::vk;
use lavashell_core::prelude::{
    Brush, Color, Density, Dp, DpExt, DrawCommand, FrameTime, PixelSize, Presented, Rect,
    SceneTree, Size, StreamId, StreamImage,
};
use lavashell_core::scene::tree::{Change, Content, Node, Packet, SceneNodeId, SceneUpdate};
use lavashell_core::surface::SurfaceChange;
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Presentation};
use skia_safe::IPoint;
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::CStr;
use std::rc::Rc;

const SURFACE: SurfaceId = SurfaceId(0);

/// Presents to surfaces without a window system, through `VK_EXT_headless_surface`.
struct HeadlessBackend;

impl PresentationBackend for HeadlessBackend {
    fn surface_extension(&self) -> Option<&'static CStr> {
        Some(ash::ext::headless_surface::NAME)
    }

    fn features(&self) -> Features {
        Features::default()
    }

    fn supports(
        &self,
        _: &PhysicalDevice<'_>,
        _: u32,
    ) -> Result<Presentation, lavashell_vulkan::Error> {
        Ok(Presentation::Supported)
    }

    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR> {
        None
    }
}

struct HeadlessWindow;

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new headless surface of `instance`, and only the `Surface` that receives it destroys it.
// A headless surface has no native window that could be destroyed before it.
unsafe impl Window for HeadlessWindow {
    fn create_surface(
        &self,
        instance: &Instance,
    ) -> Result<vk::SurfaceKHR, lavashell_vulkan::Error> {
        instance.require(&[ash::ext::headless_surface::NAME])?;
        let functions = ash::ext::headless_surface::Instance::new(instance.entry(), instance.ash());
        let info = vk::HeadlessSurfaceCreateInfoEXT::default();
        // SAFETY: ash 0.38 has no safe form of vkCreateHeadlessSurfaceEXT. The instance is alive and has the extension
        // enabled, as `require` checked, and `info` lives through the call.
        unsafe { functions.create_headless_surface(&info, None) }
            .map_err(lavashell_vulkan::Error::CreateSurface)
    }
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

/// A control that records each call made by the renderer state.
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
            .push(Event::Prepare(frame.viewport));
        Ok(())
    }

    fn before_present(&mut self, frame: &PresentFrame) {
        self.events
            .borrow_mut()
            .push(Event::BeforePresent(frame.viewport));
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

/// Streams that record when the state imports and releases them, and whose release fails.
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

/// Streams whose frames a test binds directly.
#[derive(Default)]
struct BoundFrames(StreamFrames);

impl StreamBackend for BoundFrames {
    type Gpu = VulkanRenderer;
    type Error = Infallible;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        _: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Infallible>) {
        let device = FrameDevice {
            gpu,
            painter,
            streams: &self.0,
        };
        (draw(device), Ok(()))
    }
}

fn setup() -> RendererSetup {
    RendererSetup {
        fonts: Fonts::new(),
        painter: None,
        scene: None,
        gpu_timing: false,
        full_redraw: false,
    }
}

fn state() -> Result<RendererState<RecordingControl>, TestError> {
    Ok(RendererState::new(
        &HeadlessBackend,
        setup(),
        Instant::now(),
    )?)
}

fn attach(events: &Rc<RefCell<Vec<Event>>>) -> Attach<RecordingControl> {
    Attach {
        surface: SURFACE,
        window: Arc::new(HeadlessWindow),
        alpha: SurfaceAlpha::Opaque,
        timing: SurfaceTiming::new(PresentationPolicy::Automatic),
        output: OutputRequest::default(),
        hdr_metadata: None,
        gpu_budget: None,
        control: RecordingControl {
            events: Rc::clone(events),
        },
    }
}

/// Attaches the surface with a control that records into `events`.
fn attach_recording<S: StreamBackend<Gpu = VulkanRenderer>>(
    state: &mut RendererState<RecordingControl, S>,
    events: &Rc<RefCell<Vec<Event>>>,
) -> Result<(), TestError> {
    state
        .attach(attach(events))
        .map_err(|failed| failed.error)?;
    Ok(())
}

fn frame(sequence: u64, state: &SurfaceState) -> PresentFrame<'_> {
    PresentFrame {
        time: FrameTime {
            sequence,
            ..FrameTime::default()
        },
        viewport: Viewport::new(Size::new(40.dp(), 30.dp()), Density::ONE),
        present_at: None,
        state,
    }
}

fn scene() -> Result<SceneSnapshot, TestError> {
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
                Event::Prepare(frame.viewport),
                Event::BeforePresent(frame.viewport),
                Event::AfterPresent(FrameOutcome::Shown),
            ]
        );
    }
    Ok(())
}

#[test]
fn a_surface_attaches_once_and_draws_no_more_once_retired() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = state()?;
    attach_recording(&mut state, &events)?;
    let twice = state.attach(attach(&events));
    assert!(matches!(
        twice,
        Err(AttachError {
            error: StateError::AttachedTwice(SURFACE),
            ..
        })
    ));
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
    let mut state =
        RendererState::with_streams(&HeadlessBackend, setup(), Instant::now(), |_| Ok(streams))?;
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
            Event::Prepare(frame.viewport),
            Event::ImportStreams,
            Event::BeforePresent(frame.viewport),
            Event::AfterPresent(FrameOutcome::Shown),
            Event::ReleaseStreams,
        ]
    );
    Ok(())
}

/// The area of the viewport of `frame` over which the stream is drawn, 16 by 12 pixels at density one.
const STREAM_AREA: Rect = Rect::new(Dp::new(8.0), Dp::new(6.0), Dp::new(16.0), Dp::new(12.0));

fn stream_packet(image: StreamImage) -> Packet {
    let stream = DrawCommand::StreamImage {
        image,
        destination: STREAM_AREA,
        frame: Rect::from_size(STREAM_AREA.size()),
    };
    Packet::new([stream].into())
}

/// The sRGB bytes of the pixel at `x`, `y` in the image that the frames of `SURFACE` are painted into. The image is as
/// wide as the viewport of `frame`.
fn painted_pixel<S>(
    state: &mut RendererState<RecordingControl, S>,
    x: usize,
    y: usize,
) -> Result<[u8; 4], TestError> {
    let attached = (state.surfaces.get_mut(&SURFACE)).ok_or(TestError::Missing("surface"))?;
    let pixels = (attached.target.painted_pixels()).ok_or(TestError::Missing("painted image"))?;
    let index = (y * 40 + x) * 4;
    Ok(pixels[index..index + 4].try_into()?)
}

#[test]
fn a_rebound_stream_frame_repaints_its_area_and_leaves_the_rest() -> Result<(), TestError> {
    let events = Rc::default();
    let mut state = RendererState::with_streams(&HeadlessBackend, setup(), Instant::now(), |_| {
        Ok(BoundFrames::default())
    })
    .map_err(StartError::into_state)?;
    attach_recording(&mut state, &events)?;
    let surface_state = SurfaceState::default();
    let stream = StreamId::next();
    let image = |generation| StreamImage {
        stream,
        generation,
        size: PixelSize::new(16, 12),
    };
    let background = DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 40.dp(), 30.dp()).into(),
        brush: Brush::Solid(Color::WHITE),
    };
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate::from_iter([
        insert(
            1,
            None,
            0,
            Node::Leaf(Content::Packet(Packet::new([background].into()))),
        ),
        insert(
            2,
            None,
            1,
            Node::Leaf(Content::Packet(stream_packet(image(1)))),
        ),
    ]))?;
    let mut red = state.gpu.offscreen((16, 12))?;
    let _chained_canvas = red.canvas().clear(skia_safe::Color::RED);
    (state.streams.0).bind(&image(1), red.image_snapshot().into(), IPoint::new(0, 0));
    let drawn = state.draw(SURFACE, frame(1, &surface_state), &tree.snapshot())?;
    assert_eq!(drawn.outcome, FrameOutcome::Shown);
    assert_eq!(painted_pixel(&mut state, 16, 12)?, [255, 0, 0, 255]);
    let outside = painted_pixel(&mut state, 2, 2)?;
    let mut blue = state.gpu.offscreen((16, 12))?;
    let _chained_canvas = blue.canvas().clear(skia_safe::Color::BLUE);
    (state.streams.0).bind(&image(2), blue.image_snapshot().into(), IPoint::new(0, 0));
    tree.show(SceneUpdate::from_iter([Change::SetContent {
        id: SceneNodeId(2),
        content: Content::Packet(stream_packet(image(2))),
    }]))?;
    let drawn = state.draw(SURFACE, frame(2, &surface_state), &tree.snapshot())?;
    assert_eq!(drawn.outcome, FrameOutcome::Shown);
    assert_eq!(painted_pixel(&mut state, 16, 12)?, [0, 0, 255, 255]);
    assert_eq!(painted_pixel(&mut state, 2, 2)?, outside);
    // Releases the bound images while the device of the state exists.
    state.streams.0.retire(stream);
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
    let unknown = state.commit_state(SurfaceId(1), &committed);
    assert!(matches!(
        unknown,
        Err(CommitError::State(StateError::UnknownSurface(SurfaceId(1))))
    ));
    Ok(())
}
