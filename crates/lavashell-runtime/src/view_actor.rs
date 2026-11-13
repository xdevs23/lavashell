// SPDX-License-Identifier: GPL-3.0-only
//! Runs the views of an app on the calling thread. The view actor turns the events of the platform and the reports
//! of the other actors into frames, and sends the requests that follow to the platform.

mod actors;
mod error;
mod events;
mod frames;
mod input;
mod pacing;
mod panels;
mod reports;
mod surfaces;
mod transfers;

pub use error::{Stopped, ViewError};

pub(crate) use error::StartError;

use crate::deadline_receiver::{Arrival, DeadlineReceiver};
use crate::focus_grab::FocusGrabs;
use crate::input::InputRouter;
use crate::layer_surface::LayerSurface;
use crate::observers::Observers;
use crate::output_records::OutputRecords;
use crate::panels::{PanelRecords, PanelsPerOutput};
use crate::renderer_actor::{RendererOutput, RendererStopped};
use crate::run::{RuntimeSetup, SurfaceSetup};
use crate::run_end::RunEnd;
use crate::scene_pipeline::BuilderReport;
use crate::scheduler::{Redraw, SchedulerInput, SchedulerOutput};
use crate::session_lock::{ActiveLock, LockConfiguration};
use crate::surface_records::SurfaceRecords;
use crate::view_actions::ActionQueue;
use actors::{ActorStops, Actors};
use frames::ViewSurface;
use lavashell_actor::{Clock, Closed, Effect, Merge, Outlet, Sender};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    Capability, HostFacts, LockRequest, Platform, PlatformEvent, PlatformRequest, Reports,
    StreamSetup, SurfaceRequest,
};
use lavashell_renderer::text::Shaper;
use pacing::FramePacing;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use transfers::Transfers;

/// What reaches the views from the platform and from the other actors.
pub(crate) enum ViewInput {
    Platform(PlatformEvent),
    Scheduler(SchedulerOutput),
    Renderer(RendererOutput),
    Builder(BuilderReport),
    Stopped(Stopped),
}

/// The value that an input of the views overwrites.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ViewKey {
    Platform(<PlatformEvent as Merge>::Key),
    SceneWake,
    SchedulerWake,
}

impl Merge for ViewInput {
    type Key = ViewKey;

    /// A platform event merges as [`PlatformEvent`] describes. A newer wake time of the scene source or of the frame
    /// scheduler replaces the earlier one. Every other input builds on the inputs before it.
    fn effect(&self) -> Effect<ViewKey> {
        match self {
            Self::Platform(event) => event.effect().map(ViewKey::Platform),
            Self::Scheduler(SchedulerOutput::WakeAt(_)) => {
                Effect::Overwrites(ViewKey::SchedulerWake)
            }
            Self::Renderer(RendererOutput::SceneWake(_)) => Effect::Overwrites(ViewKey::SceneWake),
            Self::Scheduler(_) | Self::Renderer(_) | Self::Builder(_) | Self::Stopped(_) => {
                Effect::BuildsOn
            }
        }
    }

    /// A platform event absorbs a later one as [`PlatformEvent`] describes. Every other later input replaces the
    /// earlier one.
    fn absorb(&mut self, later: Self) {
        match (self, later) {
            (Self::Platform(event), Self::Platform(later)) => event.absorb(later),
            (earlier, later) => *earlier = later,
        }
    }
}

/// Reports to the views that an actor stopped, once the actor drops the observer that contains it.
pub(crate) struct StopNotice {
    inbox: Sender<ViewInput>,
    stopped: Stopped,
}

impl StopNotice {
    pub(crate) fn new(inbox: &Sender<ViewInput>, stopped: Stopped) -> Self {
        Self {
            inbox: inbox.clone(),
            stopped,
        }
    }
}

impl Drop for StopNotice {
    fn drop(&mut self) {
        let _views_gone = self.inbox.push(ViewInput::Stopped(self.stopped));
    }
}

/// The link from an actor to the views, which reports the stop of the actor once the actor drops it.
struct ActorOutlet<T> {
    outlet: Outlet<T>,
    _notice: StopNotice,
}

/// The observer of an actor, which pushes each output of the actor into `inbox` as `wrap` makes it, and reports
/// `stopped` once the actor drops it.
fn observer<T: 'static>(
    inbox: &Sender<ViewInput>,
    wrap: fn(T) -> ViewInput,
    stopped: Stopped,
) -> Box<dyn FnMut(T) -> Result<(), Closed> + Send> {
    let link = ActorOutlet {
        outlet: inbox.outlet(wrap),
        _notice: StopNotice::new(inbox, stopped),
    };
    Box::new(move |output| link.push(output))
}

impl<T> ActorOutlet<T> {
    fn push(&self, output: T) -> Result<(), Closed> {
        self.outlet.push(output)
    }
}

/// The views of an app, the records of their surfaces and the actors between them and the host.
pub(crate) struct ViewActor<P: Platform> {
    records: SurfaceRecords,
    panels: PanelRecords,
    surfaces: BTreeMap<SurfaceId, ViewSurface>,
    actors: Actors<P>,
    /// Measures text for the views as the painter of the renderer shapes it.
    shaper: Shaper,
    requests: Sender<PlatformRequest>,
    host: HostFacts,
    input: InputRouter,
    outputs: OutputRecords,
    focus_grabs: FocusGrabs,
    /// Whether the platform offers accessibility. Without it, the views send no accessibility request.
    accessibility: bool,
    panels_per_output: Option<PanelsPerOutput<LayerSurface>>,
    session_lock: Option<ActiveLock>,
    transfers: Transfers,
    action_queue: ActionQueue,
    observers: Observers,
    run_end: RunEnd,
    ui_budget: Option<Duration>,
    /// The requests that follow from the events of the host, executed by the next render.
    surface_requests: Vec<SurfaceRequest>,
    pacing: FramePacing,
    clock: Arc<dyn Clock>,
}

/// What the connection to the host of `P` supplies to the views: the presentation and the streams for the renderer,
/// what the host reported when it connected, and the clock on which every actor measures time.
pub(crate) struct Connected<P: Platform> {
    pub(crate) presentation: P::Presentation,
    pub(crate) streams: StreamSetup<P::Streams>,
    pub(crate) host: HostFacts,
    pub(crate) clock: Arc<dyn Clock>,
}

impl<P: Platform> ViewActor<P> {
    /// Starts the scene pipeline, the renderer and the frame scheduler on what `connected` supplies. The scene
    /// pipeline and the frame scheduler send their reports to `inbox`. Requests the surfaces listed in `setup` from the
    /// platform through `requests`. Built-in parts whose capability is not in `offered` are not started.
    ///
    /// # Errors
    ///
    /// Fails when an actor cannot start, or when a surface of `setup` cannot be opened.
    pub(crate) fn start(
        setup: RuntimeSetup,
        offered: &BTreeSet<Capability>,
        connected: Connected<P>,
        requests: Sender<PlatformRequest>,
        inbox: &Sender<ViewInput>,
    ) -> Result<Self, StartError<P>> {
        let Connected {
            presentation,
            streams,
            host,
            clock,
        } = connected;
        let RuntimeSetup {
            surfaces,
            panels_per_output,
            session_lock,
            renderer: renderer_setup,
            ui_budget,
            run_end,
            preference_overrides,
            input_filter,
            observers,
            watches,
            clipboard,
            action_queue,
            drags,
        } = setup;
        let redraw = if observers.tick.is_some()
            || observers.frame.is_some()
            || renderer_setup.shows_scene()
        {
            Redraw::Continuous
        } else {
            Redraw::OnChange
        };
        let fonts = renderer_setup.fonts.clone();
        let cpu_timing = observers.frame_report.is_some();
        let actors = Actors::start(
            presentation,
            renderer_setup,
            streams,
            redraw,
            &clock,
            cpu_timing,
            inbox,
        )?;
        let shaper = Shaper::with_rendering(&fonts, actors.renderer.text_rendering())
            .map_err(StartError::CreateShaper)?;
        let mut actor = Self {
            records: SurfaceRecords::new(preference_overrides),
            panels: PanelRecords::default(),
            surfaces: BTreeMap::new(),
            actors,
            shaper,
            requests,
            host,
            input: InputRouter::new(input_filter),
            outputs: OutputRecords::default(),
            focus_grabs: FocusGrabs::new(offered),
            accessibility: offered.contains(&Capability::Accessibility),
            panels_per_output,
            session_lock: None,
            transfers: Transfers::new(clipboard, drags, watches, &action_queue, offered),
            action_queue,
            observers,
            run_end,
            ui_budget,
            surface_requests: Vec::new(),
            pacing: FramePacing::default(),
            clock,
        };
        actor.open_setup_surfaces(surfaces, session_lock)?;
        Ok(actor)
    }

    /// The links through which a platform reports to the actors, with `view` for the reports to the views.
    ///
    /// # Errors
    ///
    /// Fails when the renderer stopped.
    pub(crate) fn reports(
        &self,
        view: Outlet<PlatformEvent>,
    ) -> Result<Reports<P::Control, P::Streams>, RendererStopped> {
        self.actors.reports(view, Arc::clone(&self.clock))
    }

    /// Runs turns until the run ends. Each turn renders, and then waits for the inputs from the platform and the
    /// actors, or until the next deadline.
    ///
    /// # Errors
    ///
    /// Fails when a view, a request or a surface fails, or when an actor or the platform stopped.
    pub(crate) fn run(&mut self, inputs: &DeadlineReceiver<ViewInput>) -> Result<(), ViewError> {
        loop {
            self.turn()?;
            if self.finished() {
                return Ok(());
            }
            for request in self.transfers.watches.take_requests() {
                self.request(PlatformRequest::Watch(request))?;
            }
            match inputs.take_until(self.deadline())? {
                Arrival::Items(items) => {
                    for input in items {
                        self.handle(input)?;
                    }
                }
                Arrival::DeadlinePassed => {}
                Arrival::SendersGone => return Err(ViewError::Stopped(Stopped::Platform)),
            }
        }
    }

    /// Ends the run that `ran` returned. Stops the renderer, then the frame scheduler and then the scene pipeline,
    /// each after the inputs sent before. After a run that ended without a failure, the views handle the last outputs
    /// of the renderer, which reach the scheduler before it stops, and then the last outputs of the scheduler, so that
    /// the frame observer of the app receives every report. Then ends the queues of the app, so that every action,
    /// transfer and watch is cancelled, and drops the views, which ends the requests to the platform.
    ///
    /// Returns `ran`, or the failure of an output handled after it, with what each actor returned when it stopped.
    pub(crate) fn stop(
        mut self,
        inputs: &DeadlineReceiver<ViewInput>,
        ran: Result<(), ViewError>,
    ) -> (Result<(), ViewError>, ActorStops<P>) {
        let renderer = self.actors.renderer.stop();
        let ran = ran.and_then(|()| self.handle_until(inputs, Stopped::Renderer));
        let scheduler = self.actors.scheduler.stop();
        let ran = ran.and_then(|()| self.handle_until(inputs, Stopped::Scheduler));
        let scene = self.actors.scene.stop();
        self.action_queue.close();
        self.transfers.close(&mut self.records);
        let stops = ActorStops {
            renderer,
            scheduler,
            scene,
        };
        (ran, stops)
    }

    /// Handles the inputs until `part` reports that it stopped, so that every output that it sent before is handled.
    fn handle_until(
        &mut self,
        inputs: &DeadlineReceiver<ViewInput>,
        part: Stopped,
    ) -> Result<(), ViewError> {
        loop {
            let Arrival::Items(items) = inputs.take_until(None)? else {
                return Err(ViewError::Stopped(part));
            };
            let mut stopped = false;
            for input in items {
                match input {
                    ViewInput::Stopped(ended) if ended == part => stopped = true,
                    input => self.handle(input)?,
                }
            }
            if stopped {
                return Ok(());
            }
        }
    }

    /// Opens the windows and panels of the setup, and starts the session lock of `session_lock`.
    fn open_setup_surfaces(
        &mut self,
        surfaces: Vec<SurfaceSetup>,
        session_lock: Option<LockConfiguration>,
    ) -> Result<(), ViewError> {
        for surface in surfaces {
            match surface {
                SurfaceSetup::Window(window, view) => self.open_window(window, view)?,
                SurfaceSetup::Panel(options, view) => {
                    let _panel = self.open_panel(options, view, None)?;
                }
            }
        }
        if let Some(configuration) = session_lock {
            let lock = ActiveLock::start(configuration, &self.host.outputs)?;
            self.start_lock(lock)?;
        }
        Ok(())
    }

    fn turn(&mut self) -> Result<(), ViewError> {
        self.update_panels()?;
        self.update_lock()?;
        self.render()?;
        for request in self.focus_grabs.update(&self.panels, &self.records) {
            self.request(PlatformRequest::FocusGrab(request))?;
        }
        Ok(())
    }

    fn handle(&mut self, input: ViewInput) -> Result<(), ViewError> {
        match input {
            ViewInput::Platform(event) => self.platform_event(event),
            ViewInput::Scheduler(output) => {
                self.scheduler_output(output);
                Ok(())
            }
            ViewInput::Renderer(output) => self.renderer_output(output),
            ViewInput::Builder(report) => {
                self.scene_report(report);
                Ok(())
            }
            ViewInput::Stopped(stopped) => Err(stopped.into()),
        }
    }

    fn request(&self, request: PlatformRequest) -> Result<(), ViewError> {
        send_request(&self.requests, request)
    }

    /// Sends `input` to the frame scheduler. A scheduler that is gone reports its stop, which ends the run.
    fn schedule(&self, input: SchedulerInput) {
        let _gone_scheduler_ends_the_run = self.actors.scheduler.send(input);
    }

    /// The time on the clock at which the views next run without an input: the earlier of the exit time
    /// and the deadline of [`FramePacing::deadline`], or `None` without either.
    fn deadline(&mut self) -> Option<Duration> {
        let now = self.clock.now();
        let exit = (self.run_end.exit_in(now)).and_then(|wait| now.checked_add(wait));
        exit.into_iter().chain(self.pacing.deadline(now)).min()
    }

    /// Whether the run ends now. The session lock and the panels per output count as open until they end.
    fn finished(&mut self) -> bool {
        let anything_open = self.session_lock.is_some()
            || self.panels_per_output.is_some()
            || self.records.any_open();
        (self.run_end).finished(self.clock.now(), anything_open)
    }

    /// Asks the host for the session lock of `lock`, and opens a lock surface on each output known at connect. The
    /// platform reports the same outputs as added afterwards, and `open_lock_surface` skips an output with a lock
    /// surface.
    fn start_lock(&mut self, lock: ActiveLock) -> Result<(), ViewError> {
        self.request(PlatformRequest::Lock(LockRequest::Lock))?;
        self.session_lock = Some(lock);
        for output in self.host.outputs.clone() {
            self.open_lock_surface(output)?;
        }
        Ok(())
    }
}

/// Sends `request` to the platform. Blocks while the platform cannot receive more requests.
fn send_request(
    requests: &Sender<PlatformRequest>,
    request: PlatformRequest,
) -> Result<(), ViewError> {
    requests
        .push(request)
        .map_err(|Closed| ViewError::Stopped(Stopped::Platform))
}

#[cfg(test)]
mod tests;
