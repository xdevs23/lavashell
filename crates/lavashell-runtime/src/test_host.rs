// SPDX-License-Identifier: GPL-3.0-only

use crate::clipboard::ClipboardTransfers;
use crate::drag::Drags;
use crate::observers::Observers;
use crate::run::{RunError, RuntimeSetup, SurfaceSetup, drive};
use crate::run_end::RunEnd;
use crate::surface_records::SurfaceRecords;
use crate::test_error::TestError;
use crate::view_actions::ActionQueue;
use crate::watches::Watches;
use crate::window::Window;
use lavashell_actor::{Closed, SystemClock};
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::{Point, Size, Viewport};
use lavashell_core::output::OutputRequest;
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::prelude::{Modifier, NodeId, TextFieldOptions};
use lavashell_core::reactive::State;
use lavashell_core::scene::TextStyle;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp, DpExt};
use lavashell_core::view::{InputEvent, TextMeasurer, View};
use lavashell_platform::{
    Attach, Capability, Connection, HostFacts, OutputId, PainterSetup, Platform, PlatformEvent,
    PlatformLinks, PlatformRequest, RendererSetup, SurfaceAlpha, SurfaceAttachment, SurfaceControl,
    SurfaceEvent, SurfaceRequest, SurfaceSpec, TimingEvent,
};
use lavashell_renderer::{DamageMode, Fonts, FrameTiming};
use lavashell_test_fixtures::{FrameFailure, RecordedScene, WithoutFailures};
use lavashell_vulkan::HeadlessWindow;
use lavashell_vulkan_renderer::{HeadlessBackend, VulkanWindow};
use std::cell::Cell;
use std::collections::{BTreeSet, VecDeque};
use std::convert::{self, Infallible};
use std::rc::Rc;
use std::sync::{Arc, mpsc};

pub(crate) struct NoControl;

impl SurfaceControl for NoControl {
    type Error = Infallible;
    type Message = ();

    fn receive(&mut self, (): ()) {}
}

pub(crate) fn renderer_setup(painter: Option<PainterSetup>) -> RendererSetup {
    RendererSetup {
        fonts: Fonts::new(),
        painter,
        frame_timing: FrameTiming::Off,
        damage: DamageMode::Tracked,
    }
}

pub(crate) fn runtime_setup(surfaces: Vec<SurfaceSetup>) -> RuntimeSetup {
    let watches = Watches::default();
    RuntimeSetup {
        surfaces,
        panels_per_output: None,
        session_lock: None,
        renderer: renderer_setup(None),
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

pub(crate) fn end_on_marker(setup: &mut RuntimeSetup) {
    let marked = Rc::new(Cell::new(false));
    let observed = Rc::clone(&marked);
    setup.observers.frame_report = Some(Box::new(move |_: FrameReport| observed.set(true)));
    setup.run_end.exit_when(move || marked.get());
}

pub(crate) fn host(outputs: impl IntoIterator<Item = OutputId>) -> HostFacts {
    HostFacts {
        outputs: outputs.into_iter().collect(),
        reports_presentation: true,
    }
}

pub(crate) fn surface_requests(requests: &[PlatformRequest]) -> Vec<SurfaceRequest> {
    (requests.iter())
        .filter_map(|request| match request {
            PlatformRequest::Surface(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

pub(crate) fn created(requests: &[PlatformRequest]) -> Vec<(SurfaceId, SurfaceSpec)> {
    (surface_requests(requests).into_iter())
        .filter_map(|request| match request {
            SurfaceRequest::Create { surface, spec, .. } => Some((surface, *spec)),
            SurfaceRequest::Destroy(_) | SurfaceRequest::Release(_) => None,
        })
        .collect()
}

pub(crate) fn empty_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

pub(crate) fn window(title: &str) -> SurfaceSetup {
    let window = Window::new(title, "test.app").size(100.dp(), 100.dp());
    SurfaceSetup::Window(window, empty_view())
}

pub(crate) struct Metrics;

impl TextMeasurer for Metrics {
    fn measure(&mut self, value: &str, _: &TextStyle<Dp>, width: Dp) -> Size {
        let natural: Dp = value.chars().map(|_| 10.dp()).sum();
        Size::new(natural.min(width), 20.dp())
    }
}

pub(crate) const FIELD_VIEWPORT: Viewport =
    Viewport::new(Size::new(Dp::new(220.0), Dp::new(100.0)), Density::ONE);

pub(crate) struct FieldView {
    pub(crate) view: View,
    pub(crate) scene: RecordedScene,
    pub(crate) value: State<String>,
}

pub(crate) fn field_view(modifier: Modifier) -> Result<FieldView, FrameFailure> {
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
    Ok(FieldView { view, scene, value })
}

pub(crate) fn field_id(view: &View) -> Result<NodeId, TestError> {
    (view.accessibility().nodes.values())
        .find(|node| node.name.as_deref() == Some("field"))
        .map(|node| node.id)
        .ok_or(TestError::Missing("the field"))
}

pub(crate) struct OpenSurface {
    pub(crate) watches: Watches,
    pub(crate) records: SurfaceRecords,
    pub(crate) surface: SurfaceId,
}

pub(crate) fn open_empty_surface() -> Result<OpenSurface, TestError> {
    let watches = Watches::default();
    let mut records = SurfaceRecords::new(PreferenceOverrides::default());
    let size = Size::new(100.dp(), 50.dp());
    let surface = records.open(empty_view(), size, Density::ONE, None)?;
    Ok(OpenSurface {
        watches,
        records,
        surface,
    })
}

pub(crate) fn focus_field(view: &mut View) {
    view.input(InputEvent::Press(Point::new(11.dp(), 5.dp())));
    view.input(InputEvent::Release(Point::new(11.dp(), 5.dp())));
}

pub(crate) fn closed(surface: SurfaceId) -> PlatformEvent {
    PlatformEvent::Surface(surface, SurfaceEvent::Closed)
}

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
}

type Script = Box<dyn FnOnce(Wire) -> Result<(), StandInError> + Send>;

type RequestLog = mpsc::Sender<Vec<PlatformRequest>>;

pub(crate) enum Scenario {
    Unreachable,
    Scripted {
        backend: Option<HeadlessBackend>,
        host: HostFacts,
        offered: BTreeSet<Capability>,
        script: Script,
        log: RequestLog,
    },
}

pub(crate) struct StandIn {
    offered: BTreeSet<Capability>,
    script: Script,
    log: RequestLog,
}

impl Platform for StandIn {
    type Setup = Scenario;
    type Error = StandInError;
    type Backend = HeadlessBackend;
    type Control = NoControl;

    fn connect(scenario: Scenario) -> Result<Connection<Self>, StandInError> {
        match scenario {
            Scenario::Scripted {
                backend,
                host,
                offered,
                script,
                log,
            } => Ok(Connection {
                platform: Self {
                    offered,
                    script,
                    log,
                },
                backend,
                host,
                clock: Arc::new(SystemClock::start()),
            }),
            Scenario::Unreachable => Err(StandInError::Unreachable),
        }
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        self.offered.clone()
    }

    fn serve(self, links: PlatformLinks<NoControl, HeadlessBackend>) -> Result<(), StandInError> {
        (self.script)(Wire::new(links, self.log))
    }
}

pub(crate) struct Wire {
    links: PlatformLinks<NoControl, HeadlessBackend>,
    waiting: VecDeque<PlatformRequest>,
    taken: Vec<PlatformRequest>,
    log: RequestLog,
}

impl Wire {
    fn new(links: PlatformLinks<NoControl, HeadlessBackend>, log: RequestLog) -> Self {
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

    pub(crate) fn attach(&self, surface: SurfaceId) -> Result<(), StandInError> {
        let window: VulkanWindow = Arc::new(HeadlessWindow);
        let attach = SurfaceAttachment::Attach(Box::new(Attach {
            surface,
            window,
            alpha: SurfaceAlpha::Opaque,
            output: OutputRequest::default(),
            hdr_metadata: None,
            control: NoControl,
        }));
        self.links
            .reports
            .renderer
            .push(attach)
            .map_err(StandInError::from)
    }

    pub(crate) fn detach(&self, surface: SurfaceId) -> Result<(), StandInError> {
        let detach = SurfaceAttachment::Detach(surface);
        self.links
            .reports
            .renderer
            .push(detach)
            .map_err(StandInError::from)
    }

    pub(crate) fn end_marker(&self) -> Result<(), StandInError> {
        self.timing(TimingEvent::Discarded {
            surface: SurfaceId::numbered(0),
            frame: FrameTime::default(),
        })
    }

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

    pub(crate) fn finish(mut self) -> Result<(), StandInError> {
        while self.next_request().is_some() {}
        self.log.send(self.taken)?;
        Ok(())
    }

    fn next_request(&mut self) -> Option<&PlatformRequest> {
        if self.waiting.is_empty() {
            self.waiting.extend(self.links.requests.take().ok()?);
        }
        let request = self.waiting.pop_front()?;
        self.taken.push(request);
        self.taken.last()
    }
}

pub(crate) type Outcome = Result<(), RunError<StandIn>>;

pub(crate) struct Run {
    pub(crate) outcome: Outcome,
    requests: mpsc::Receiver<Vec<PlatformRequest>>,
}

impl Run {
    pub(crate) fn requests(self) -> Result<Vec<PlatformRequest>, TestError> {
        self.outcome?;
        Ok(self.requests.recv()?)
    }
}

pub(crate) fn run_scripted(
    host: HostFacts,
    offered: impl IntoIterator<Item = Capability>,
    setup: RuntimeSetup,
    script: impl FnOnce(Wire) -> Result<(), StandInError> + Send + 'static,
) -> Run {
    let backend = HeadlessBackend::without_surfaces();
    run_scripted_on(Some(backend), host, offered, setup, script)
}

pub(crate) fn run_scripted_on(
    backend: Option<HeadlessBackend>,
    host: HostFacts,
    offered: impl IntoIterator<Item = Capability>,
    setup: RuntimeSetup,
    script: impl FnOnce(Wire) -> Result<(), StandInError> + Send + 'static,
) -> Run {
    let (log, requests) = mpsc::channel();
    let scenario = Scenario::Scripted {
        backend,
        host,
        offered: offered.into_iter().collect(),
        script: Box::new(script),
        log,
    };
    Run {
        outcome: drive::<StandIn>(setup, scenario, convert::identity),
        requests,
    }
}

pub(crate) fn run_unreachable(setup: RuntimeSetup) -> Outcome {
    drive::<StandIn>(setup, Scenario::Unreachable, convert::identity)
}
