// SPDX-License-Identifier: GPL-3.0-only
//! A scripted platform for the tests that run the actors of the runtime.

use crate::clipboard::ClipboardTransfers;
use crate::drag::Drags;
use crate::observers::Observers;
use crate::run::{RunError, RuntimeSetup, SurfaceSetup, drive};
use crate::run_end::RunEnd;
use crate::test_error::TestError;
use crate::view_actions::ActionQueue;
use crate::watches::Watches;
use crate::window::Window;
use lavashell_actor::{Closed, SystemClock};
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::{Point, Size, Viewport};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::prelude::{Modifier, TextFieldOptions};
use lavashell_core::reactive::State;
use lavashell_core::scene::TextStyle;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::units::{Density, Dp, DpExt};
use lavashell_core::view::{InputEvent, TextMeasurer, View};
use lavashell_platform::{
    Capability, Connection, HostFacts, OutputId, Platform, PlatformEvent, PlatformLinks,
    PlatformRequest, SurfaceAttachment, SurfaceEvent, SurfaceRequest, SurfaceSpec, TimingEvent,
};
use lavashell_renderer::{
    Fonts, FrameOutcome, HeadlessPresentation, PainterSetup, PresentFrame, RendererSetup,
    SurfaceControl,
};
use lavashell_test_fixtures::{FrameFailure, RecordedScene, WithoutFailures};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, VecDeque};
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::{Arc, mpsc};

/// A control for a surface that is never attached.
pub(crate) struct NoControl;

impl SurfaceControl for NoControl {
    type Error = Infallible;
    type Message = ();

    fn receive(&mut self, (): ()) {}

    fn prepare(&mut self, _: &PresentFrame) -> Result<(), Infallible> {
        Ok(())
    }

    fn before_present(&mut self, _: &PresentFrame) {}

    fn after_present(&mut self, _: FrameOutcome) {}

    fn commit_state(&mut self, _: &SurfaceState) -> Result<(), Infallible> {
        Ok(())
    }
}

pub(crate) fn renderer_setup(painter: Option<PainterSetup>) -> RendererSetup {
    RendererSetup {
        fonts: Fonts::new(),
        painter,
        scene: None,
        gpu_timing: false,
        full_redraw: false,
    }
}

/// A setup that opens `surfaces` and has no panels per output, session lock, observers or end condition.
pub(crate) fn runtime_setup(surfaces: Vec<SurfaceSetup>) -> RuntimeSetup {
    let watches = Watches::default();
    RuntimeSetup {
        surfaces,
        panels_per_output: None,
        session_lock: None,
        renderer: renderer_setup(None),
        ui_budget: None,
        run_end: RunEnd::default(),
        preference_overrides: PreferenceOverrides::default(),
        input_filter: None,
        observers: Observers::default(),
        clipboard: ClipboardTransfers::new(&watches),
        action_queue: ActionQueue::new(&watches),
        drags: Drags::new(&watches),
        watches,
    }
}

/// Ends the run once the frame observer of `setup` receives a report, which the platform triggers with
/// `Wire::end_marker`.
pub(crate) fn end_on_marker(setup: &mut RuntimeSetup) {
    let marked = Rc::new(Cell::new(false));
    let observed = Rc::clone(&marked);
    setup.observers.frame_report = Some(Box::new(move |_: FrameReport| observed.set(true)));
    setup.run_end.exit_when(move || marked.get());
}

/// A host with `outputs` that reports when it presents frames.
pub(crate) fn host(outputs: impl IntoIterator<Item = OutputId>) -> HostFacts {
    HostFacts {
        outputs: outputs.into_iter().collect(),
        reports_presentation: true,
    }
}

/// The surface requests among `requests`, in order.
pub(crate) fn surface_requests(requests: &[PlatformRequest]) -> Vec<SurfaceRequest> {
    (requests.iter())
        .filter_map(|request| match request {
            PlatformRequest::Surface(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

/// The surface and the spec of each surface that `requests` ask for, in order.
pub(crate) fn created(requests: &[PlatformRequest]) -> Vec<(SurfaceId, SurfaceSpec)> {
    (surface_requests(requests).into_iter())
        .filter_map(|request| match request {
            SurfaceRequest::Create { surface, spec, .. } => Some((surface, *spec)),
            SurfaceRequest::Destroy(_) | SurfaceRequest::Release(_) => None,
        })
        .collect()
}

/// A view that shows nothing.
pub(crate) fn empty_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

/// A window of 100 by 100 dp that shows an empty view.
pub(crate) fn window(title: &str) -> SurfaceSetup {
    let window = Window::new(title, "test.app").size(100.dp(), 100.dp());
    SurfaceSetup::Window(window, empty_view())
}

struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = value.chars().map(|_| 10.dp()).sum();
        Size::new(natural.min(width), 20.dp())
    }
}

pub(crate) const FIELD_VIEWPORT: Viewport =
    Viewport::new(Size::new(Dp::new(220.0), Dp::new(100.0)), Density::ONE);

/// A view that shows a text field named `field` with the text "abc def", after its first frame, and the state of the
/// text.
pub(crate) fn field_view(modifier: Modifier) -> Result<(View, State<String>), FrameFailure> {
    let value = State::new("abc def".to_string());
    let shown = value.clone();
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let shown = shown.clone();
        ui.text_area(
            shown.get(),
            TextFieldOptions::default(),
            modifier
                .clone()
                .accessible_name("field")
                .size(140.dp(), 60.dp()),
            move |text| shown.set(text),
        );
    });
    let mut scene = RecordedScene::default();
    (view.frame_into(FIELD_VIEWPORT, &mut Metrics, &mut scene)).without_failures()?;
    Ok((view, value))
}

/// Focuses the text field of a view made by [`field_view`] with a press and a release on it.
pub(crate) fn focus_field(view: &mut View) {
    view.input(InputEvent::Press(Point::new(11.dp(), 5.dp())));
    view.input(InputEvent::Release(Point::new(11.dp(), 5.dp())));
}

/// The event of a host that closed `surface`.
pub(crate) fn closed(surface: SurfaceId) -> PlatformEvent {
    PlatformEvent::Surface(surface, SurfaceEvent::Closed)
}

/// Why a scripted platform failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StandInError {
    #[error("the host cannot be reached")]
    Unreachable,
    #[error("the platform failed while it served")]
    Failed,
    #[error("the runtime ended its requests before the request that the script awaited")]
    RequestsEnded,
    #[error("the runtime does not receive from the platform anymore")]
    LinkClosed(#[from] Closed),
    #[error("the test does not receive the requests of the platform anymore")]
    LogClosed(#[from] mpsc::SendError<Vec<PlatformRequest>>),
    #[error("the test scripted no platform")]
    Unscripted,
}

type Script = Box<dyn FnOnce(Wire) -> Result<(), StandInError> + Send>;

type RequestLog = mpsc::Sender<Vec<PlatformRequest>>;

enum Scenario {
    Unreachable,
    Scripted {
        host: HostFacts,
        offered: BTreeSet<Capability>,
        script: Script,
        log: RequestLog,
    },
}

thread_local! {
    static SCENARIO: RefCell<Option<Scenario>> = const { RefCell::new(None) };
}

/// A platform that runs the script of a test on the platform thread instead of connecting to a host. The test sets
/// the script with [`run_scripted`] on the thread that calls `drive`.
pub(crate) struct StandIn {
    offered: BTreeSet<Capability>,
    script: Script,
    log: RequestLog,
}

impl Platform for StandIn {
    type Error = StandInError;
    type Presentation = HeadlessPresentation;
    type Control = NoControl;

    fn connect() -> Result<Connection<Self>, StandInError> {
        match SCENARIO.with_borrow_mut(Option::take) {
            Some(Scenario::Scripted {
                host,
                offered,
                script,
                log,
            }) => Ok(Connection {
                platform: Self {
                    offered,
                    script,
                    log,
                },
                presentation: HeadlessPresentation::without_surfaces(),
                host,
                clock: Arc::new(SystemClock::start()),
            }),
            Some(Scenario::Unreachable) => Err(StandInError::Unreachable),
            None => Err(StandInError::Unscripted),
        }
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        self.offered.clone()
    }

    fn serve(self, links: PlatformLinks<NoControl>) -> Result<(), StandInError> {
        (self.script)(Wire::new(links, self.log))
    }
}

/// The links of a scripted platform to the runtime, and the requests that the script took from the runtime.
pub(crate) struct Wire {
    links: PlatformLinks<NoControl>,
    waiting: VecDeque<PlatformRequest>,
    taken: Vec<PlatformRequest>,
    log: RequestLog,
}

impl Wire {
    fn new(links: PlatformLinks<NoControl>, log: RequestLog) -> Self {
        Self {
            links,
            waiting: VecDeque::new(),
            taken: Vec::new(),
            log,
        }
    }

    pub(crate) fn event(&self, event: PlatformEvent) -> Result<(), StandInError> {
        self.links
            .reports
            .view
            .push(event)
            .map_err(StandInError::from)
    }

    pub(crate) fn timing(&self, event: TimingEvent) -> Result<(), StandInError> {
        self.links
            .reports
            .scheduler
            .push(event)
            .map_err(StandInError::from)
    }

    /// Detaches `surface` from the renderer, which then reports that it retired the surface.
    pub(crate) fn detach(&self, surface: SurfaceId) -> Result<(), StandInError> {
        let detach = SurfaceAttachment::Detach(surface);
        self.links
            .reports
            .renderer
            .push(detach)
            .map_err(StandInError::from)
    }

    /// Reports a discarded frame to the frame scheduler. The scheduler passes the report to the frame observer of the
    /// app after the events that the platform sent before.
    pub(crate) fn end_marker(&self) -> Result<(), StandInError> {
        self.timing(TimingEvent::Discarded {
            surface: SurfaceId(0),
            frame: FrameTime::default(),
        })
    }

    /// Takes requests of the runtime until the surface request `wanted` arrives.
    pub(crate) fn await_surface_request(
        &mut self,
        wanted: &SurfaceRequest,
    ) -> Result<(), StandInError> {
        loop {
            let request = self.next_request().ok_or(StandInError::RequestsEnded)?;
            if matches!(request, PlatformRequest::Surface(surface) if surface == wanted) {
                return Ok(());
            }
        }
    }

    /// Takes the requests of the runtime until the runtime ends them, and hands every request that the script took to
    /// the test.
    pub(crate) fn finish(mut self) -> Result<(), StandInError> {
        while self.next_request().is_some() {}
        self.log.send(self.taken)?;
        Ok(())
    }

    /// The next request of the runtime, or `None` once the runtime ended its requests.
    fn next_request(&mut self) -> Option<&PlatformRequest> {
        if self.waiting.is_empty() {
            self.waiting.extend(self.links.requests.take().ok()?);
        }
        let request = self.waiting.pop_front()?;
        self.taken.push(request);
        self.taken.last()
    }
}

pub(crate) type Outcome = Result<(), RunError<StandInError, Infallible>>;

/// The outcome of a run on a scripted platform.
pub(crate) struct Run {
    pub(crate) outcome: Outcome,
    requests: mpsc::Receiver<Vec<PlatformRequest>>,
}

impl Run {
    /// The requests that the script took from the runtime. Fails with the failure of the run, or when the script
    /// ended without finishing.
    pub(crate) fn requests(self) -> Result<Vec<PlatformRequest>, TestError> {
        self.outcome?;
        Ok(self.requests.recv()?)
    }
}

/// Runs `setup` on a platform that connects to `host`, offers `offered` and runs `script` on the platform thread.
pub(crate) fn run_scripted(
    host: HostFacts,
    offered: impl IntoIterator<Item = Capability>,
    setup: RuntimeSetup,
    script: impl FnOnce(Wire) -> Result<(), StandInError> + Send + 'static,
) -> Run {
    let (log, requests) = mpsc::channel();
    SCENARIO.set(Some(Scenario::Scripted {
        host,
        offered: offered.into_iter().collect(),
        script: Box::new(script),
        log,
    }));
    Run {
        outcome: drive::<StandIn>(setup),
        requests,
    }
}

/// Runs `setup` on a platform whose host cannot be reached.
pub(crate) fn run_unreachable(setup: RuntimeSetup) -> Outcome {
    SCENARIO.set(Some(Scenario::Unreachable));
    drive::<StandIn>(setup)
}
