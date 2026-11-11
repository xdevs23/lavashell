// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use crate::test_host::{
    NoControl, closed, created, empty_view, host, runtime_setup, surface_requests, window,
};
use crate::watches::AfterReady;
use crate::window::Window;
use lavashell_actor::{Receiver, SystemClock, mailbox};
use lavashell_core::geometry::Size;
use lavashell_core::output::OutputRequest;
use lavashell_core::units::DpExt;
use lavashell_platform::{
    Appearance, Readiness, Role, SurfaceSpec, WatchEvent, WatchId, WatchRequest, WindowRole,
};
use lavashell_renderer::HeadlessPresentation;
use rustix::event::{EventfdFlags, eventfd};
use std::cell::Cell;
use std::rc::Rc;

const WINDOW: SurfaceId = SurfaceId(0);

/// A view actor that a test hands one input at a time, with the requests that it sends to the platform.
struct Views {
    actor: ViewActor<NoControl>,
    inbox: Sender<ViewInput>,
    inputs: DeadlineReceiver<ViewInput>,
    requests: Receiver<PlatformRequest>,
    clock: Arc<dyn Clock>,
}

impl Views {
    fn start(
        setup: RuntimeSetup,
        offered: impl IntoIterator<Item = Capability>,
        host: HostFacts,
    ) -> Result<Self, TestError> {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::start());
        let (inbox, items) = mailbox();
        let inputs = DeadlineReceiver::new(items, clock.as_ref()).map_err(ViewError::from)?;
        let (sender, requests) = mailbox();
        let offered = offered.into_iter().collect();
        let presentation = HeadlessPresentation::without_surfaces();
        let actor = ViewActor::<NoControl>::start(
            setup,
            &offered,
            presentation,
            host,
            &clock,
            sender,
            &inbox,
        )?;
        Ok(Self {
            actor,
            inbox,
            inputs,
            requests,
            clock,
        })
    }

    /// Hands `events` to the views and runs one turn.
    fn play(&mut self, events: impl IntoIterator<Item = PlatformEvent>) -> Result<(), TestError> {
        for event in events {
            self.actor.handle(ViewInput::Platform(event))?;
        }
        self.actor.turn()?;
        Ok(())
    }

    /// Plays `events`, and returns the requests that the views sent since the last call.
    fn step(
        &mut self,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<Vec<PlatformRequest>, TestError> {
        self.play(events)?;
        Ok(self.requests.take_arrived()?)
    }

    /// Queues `events`, runs the views until the run ends, and returns the requests that the views sent.
    fn run(
        &mut self,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<Vec<PlatformRequest>, TestError> {
        for event in events {
            self.inbox.push(ViewInput::Platform(event))?;
        }
        self.actor.run(&self.inputs)?;
        Ok(self.requests.take_arrived()?)
    }
}

fn views_of(
    surfaces: Vec<SurfaceSetup>,
    offered: impl IntoIterator<Item = Capability>,
) -> Result<Views, TestError> {
    Views::start(runtime_setup(surfaces), offered, host([]))
}

fn wake_at(wake: Duration) -> ViewInput {
    ViewInput::Scheduler(SchedulerOutput::WakeAt(Some(wake)))
}

fn scene_wake_at(deadline: Duration) -> ViewInput {
    ViewInput::Renderer(RendererOutput::SceneWake(Some(deadline)))
}

#[test]
fn a_window_is_requested_as_its_description_says() -> TestResult {
    let described = Window::new("player", "test.app")
        .size(320.dp(), 240.dp())
        .min_size(200.dp(), 100.dp())
        .fullscreen(true);
    let budget = Duration::from_millis(4);
    let mut setup = runtime_setup(vec![SurfaceSetup::Window(described, empty_view())]);
    setup.ui_budget = Some(budget);
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    let expected = SurfaceRequest::Create {
        surface: WINDOW,
        spec: Box::new(SurfaceSpec {
            size: Size::new(320.dp(), 240.dp()),
            appearance: Appearance {
                transparent: false,
                output: OutputRequest::default(),
                hdr_metadata: None,
                background_blur: false,
            },
            role: Role::Window(WindowRole {
                title: "player".to_owned(),
                app_id: "test.app".to_owned(),
                min_size: Some(Size::new(200.dp(), 100.dp())),
                fullscreen: true,
                draws_decorations: false,
            }),
        }),
        gpu_budget: Some(budget),
    };
    assert_eq!(surface_requests(&views.step([])?), [expected]);
    Ok(())
}

#[test]
fn the_surfaces_of_the_setup_are_requested_in_order_under_consecutive_ids() -> TestResult {
    let mut views = views_of(vec![window("first"), window("second")], Capability::ALL)?;
    let requests = views.step([])?;
    assert!(matches!(
        created(&requests).as_slice(),
        [
            (SurfaceId(0), SurfaceSpec { role: Role::Window(first), .. }),
            (SurfaceId(1), SurfaceSpec { role: Role::Window(second), .. }),
        ] if first.title == "first" && second.title == "second"
    ));
    Ok(())
}

#[test]
fn a_surface_closed_by_the_host_is_destroyed_once_and_ends_the_run() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    assert!(!views.actor.finished());
    let requests = views.step([closed(WINDOW), closed(WINDOW)])?;
    assert!(matches!(
        surface_requests(&requests).as_slice(),
        [
            SurfaceRequest::Create {
                surface: WINDOW,
                ..
            },
            SurfaceRequest::Destroy(WINDOW)
        ]
    ));
    assert!(views.actor.finished());
    Ok(())
}

#[test]
fn a_close_of_a_surface_that_is_unknown_changes_nothing() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    let requests = views.step([closed(SurfaceId(7))])?;
    assert!(matches!(
        surface_requests(&requests).as_slice(),
        [SurfaceRequest::Create {
            surface: WINDOW,
            ..
        }]
    ));
    assert!(!views.actor.finished());
    Ok(())
}

#[test]
fn the_stop_of_an_actor_ends_the_views_with_the_error_of_that_actor() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    let mut stop = |stopped| views.actor.handle(ViewInput::Stopped(stopped));
    let parts = [
        Stopped::Scene,
        Stopped::Renderer,
        Stopped::Scheduler,
        Stopped::Platform,
    ];
    for stopped in parts {
        assert!(matches!(stop(stopped), Err(ViewError::Stopped(part)) if part == stopped));
    }
    Ok(())
}

#[test]
fn a_run_ends_with_the_error_of_an_actor_that_stopped() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    views.inbox.push(ViewInput::Stopped(Stopped::Scheduler))?;
    let ended = views.actor.run(&views.inputs);
    assert!(matches!(ended, Err(ViewError::Stopped(Stopped::Scheduler))));
    Ok(())
}

#[test]
fn a_stop_notice_reports_its_actor_when_it_is_dropped() -> TestResult {
    let (inbox, items) = mailbox();
    drop(StopNotice::new(&inbox, Stopped::Scheduler));
    drop(inbox);
    let taken = items.take()?;
    assert!(matches!(
        taken.as_slice(),
        [ViewInput::Stopped(Stopped::Scheduler)]
    ));
    Ok(())
}

#[test]
fn a_newer_wake_time_replaces_an_earlier_one_and_other_inputs_keep_their_order() -> TestResult {
    let (inbox, items) = mailbox();
    inbox.push(wake_at(Duration::from_millis(1)))?;
    inbox.push(ViewInput::Renderer(RendererOutput::SceneWake(None)))?;
    inbox.push(wake_at(Duration::from_millis(2)))?;
    inbox.push(ViewInput::Scheduler(SchedulerOutput::CommitState(WINDOW)))?;
    inbox.push(wake_at(Duration::from_millis(3)))?;
    let two = Some(Duration::from_millis(2));
    let three = Some(Duration::from_millis(3));
    let taken = items.take()?;
    assert!(matches!(
        taken.as_slice(),
        [
            ViewInput::Scheduler(SchedulerOutput::WakeAt(second)),
            ViewInput::Renderer(RendererOutput::SceneWake(None)),
            ViewInput::Scheduler(SchedulerOutput::CommitState(WINDOW)),
            ViewInput::Scheduler(SchedulerOutput::WakeAt(third)),
        ] if *second == two && *third == three
    ));
    Ok(())
}

#[test]
fn a_wake_time_that_is_due_ends_the_wait_only_once() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    assert_eq!(views.actor.deadline(), None);
    views.actor.handle(wake_at(Duration::ZERO))?;
    let due = views.actor.deadline();
    assert!(due.is_some_and(|at| at <= views.clock.now()));
    assert_eq!(views.actor.deadline(), None);
    Ok(())
}

#[test]
fn the_wait_ends_at_the_earliest_scene_deadline_or_wake_time_that_is_still_ahead() -> TestResult {
    let mut views = views_of(vec![window("window")], Capability::ALL)?;
    let passed = views.clock.now();
    let ahead = passed + Duration::from_secs(500);
    views.actor.handle(scene_wake_at(passed))?;
    assert_eq!(views.actor.deadline(), None);
    views.actor.handle(scene_wake_at(ahead))?;
    views.actor.handle(wake_at(Duration::from_secs(5000)))?;
    assert_eq!(views.actor.deadline(), Some(ahead));
    Ok(())
}

#[test]
fn the_exit_time_of_the_run_ends_the_wait_unless_a_wake_time_is_due_sooner() -> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    setup.run_end.exit_after(Duration::from_secs(5000));
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    let exit = views.actor.deadline();
    assert!(exit.is_some_and(|at| at > views.clock.now()));
    let sooner = Duration::from_secs(10);
    views.actor.handle(wake_at(sooner))?;
    assert_eq!(views.actor.deadline(), Some(sooner));
    Ok(())
}

#[test]
fn a_watch_of_the_app_is_requested_from_the_platform_and_a_ready_report_runs_it() -> TestResult {
    let setup = runtime_setup(vec![window("window")]);
    let ran = Rc::new(Cell::new(false));
    let seen = Rc::clone(&ran);
    let on_ready = move |_: WatchId| {
        seen.set(true);
        AfterReady::End
    };
    let descriptor = eventfd(0, EventfdFlags::CLOEXEC)?;
    let watches = &setup.watches;
    let watch = watches.watch(descriptor, Readiness::Readable, on_ready)?;
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    let ready = PlatformEvent::Watch(WatchEvent::Ready(watch));
    let requests = views.run([ready, closed(WINDOW)])?;
    assert!(ran.get());
    assert!(requests.iter().any(|request| matches!(
        request,
        PlatformRequest::Watch(WatchRequest::Watch(id, _, Readiness::Readable)) if *id == watch
    )));
    Ok(())
}

mod input;
mod outputs;
