// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::PresentationPolicy;
use crate::test_error::TestError;
use ash::vk;
use lavashell_core::prelude::{
    Brush, Color, Density, DpExt, DrawCommand, FrameTime, Rect, SceneTree, Size,
};
use lavashell_core::surface::SurfaceChange;
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Presentation};
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
    CommitState(SurfaceState),
}

/// A control that records each call made by the renderer state.
struct RecordingControl {
    events: Rc<RefCell<Vec<Event>>>,
}

impl SurfaceControl for RecordingControl {
    type Error = Infallible;

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

fn state() -> Result<RendererState<RecordingControl>, TestError> {
    let setup = RendererSetup {
        fonts: Fonts::new(),
        painter: None,
        scene: None,
        gpu_timing: false,
        full_redraw: false,
        start: Instant::now(),
    };
    Ok(RendererState::new(&HeadlessBackend, setup)?)
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
fn attach_recording(
    state: &mut RendererState<RecordingControl>,
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
        Err(DrawError::State(StateError::UnknownSurface(SURFACE)))
    ));
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
