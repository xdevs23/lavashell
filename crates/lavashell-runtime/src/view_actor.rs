// SPDX-License-Identifier: GPL-3.0-only

mod actors;
mod error;
mod events;
mod frames;
mod input;
mod open_surfaces;
mod outputs;
mod pacing;
mod panels;
mod reports;
mod surfaces;
mod transfers;

pub use error::{Stopped, ViewError};
pub(crate) use frames::ComposedFrame;

use crate::controls_actor::ControlsOutput;
use crate::deadline_receiver::{Arrival, DeadlineReceiver};
use crate::input::InputRouter;
use crate::observers::Observers;
use crate::renderer_actor::{RendererOutput, RendererStopped};
use crate::run::{RunError, RuntimeSetup, SurfaceSetup};
use crate::run_end::RunEnd;
use crate::scene_pipeline::BuilderReport;
use crate::scheduler::{Redraw, SchedulerInput, SchedulerOutput};
use crate::session_lock::LockConfiguration;
use crate::view_actions::ActionQueue;
use crate::window_requests::to_window_control_request;
use actors::{ActorSetup, ActorStops, Actors, AttachedSurfaces};
use lavashell_actor::{Clock, Closed, Merge, MergeEffect, Outlet, Sender};
use lavashell_platform::{
    Capability, HostFacts, Platform, PlatformEvent, PlatformRequest, RendererBackend, Reports,
    SessionLockRequest,
};
use lavashell_renderer::text::Shaper;
use open_surfaces::OpenSurfaces;
use outputs::Outputs;
use pacing::FramePacing;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use transfers::Transfers;

pub(crate) enum ViewInput {
    Platform(PlatformEvent),
    Scheduler(SchedulerOutput),
    Renderer(RendererOutput),
    Controls(ControlsOutput),
    Builder(BuilderReport),
    Stopped(Stopped),
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum ViewKey {
    Platform(<PlatformEvent as Merge>::Key),
    SceneWake,
    SchedulerWake,
}

impl Merge for ViewInput {
    type Key = ViewKey;

    fn effect(&self) -> MergeEffect<ViewKey> {
        match self {
            Self::Platform(event) => event.effect().map(ViewKey::Platform),
            Self::Scheduler(SchedulerOutput::WakeAt(_)) => {
                MergeEffect::Overwrites(ViewKey::SchedulerWake)
            }
            Self::Renderer(RendererOutput::SceneWake(_)) => {
                MergeEffect::Overwrites(ViewKey::SceneWake)
            }
            Self::Scheduler(_)
            | Self::Renderer(_)
            | Self::Controls(_)
            | Self::Builder(_)
            | Self::Stopped(_) => MergeEffect::Appends,
        }
    }

    fn absorb(&mut self, later: Self) {
        match (self, later) {
            (Self::Platform(event), Self::Platform(later)) => event.absorb(later),
            (earlier, later) => *earlier = later,
        }
    }
}

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

struct ActorOutlet<T> {
    outlet: Outlet<T>,
    _notice: StopNotice,
}

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

pub(crate) struct ViewActor<P: Platform> {
    surfaces: OpenSurfaces,
    outputs: Outputs,
    actors: Actors<P>,
    shaper: Shaper,
    requests: Sender<PlatformRequest>,
    input: InputRouter,
    accessibility: bool,
    transfers: Transfers,
    action_queue: ActionQueue,
    observers: Observers,
    run_end: RunEnd,
    pacing: FramePacing,
    clock: Arc<dyn Clock>,
}

pub(crate) struct Connected<P: Platform> {
    pub(crate) backend: Option<P::Backend>,
    pub(crate) host: HostFacts,
    pub(crate) clock: Arc<dyn Clock>,
}

impl<P: Platform> ViewActor<P> {
    pub(crate) fn start(
        setup: RuntimeSetup,
        offered: &BTreeSet<Capability>,
        connected: Connected<P>,
        requests: Sender<PlatformRequest>,
        inbox: &Sender<ViewInput>,
    ) -> Result<Self, RunError<P>> {
        let Connected {
            backend,
            host,
            clock,
        } = connected;
        let RuntimeSetup {
            surfaces,
            panels_per_output,
            session_lock,
            renderer: renderer_setup,
            run_end,
            preference_overrides,
            input_filter,
            observers,
            watches,
            clipboard,
            action_queue,
            drags,
        } = setup;
        let shows_scene = (backend.as_ref()).is_some_and(RendererBackend::shows_scene);
        let redraw = if observers.tick.is_some() || observers.frame.is_some() || shows_scene {
            Redraw::Continuous
        } else {
            Redraw::OnChange
        };
        let fonts = renderer_setup.fonts.clone();
        let actor_setup = ActorSetup {
            backend,
            renderer: renderer_setup,
            redraw,
            cpu_timing: observers.cpu_timing(),
        };
        let (actors, text_rendering) = Actors::start(actor_setup, &clock, inbox)?;
        let shaper =
            Shaper::with_rendering(&fonts, text_rendering).map_err(RunError::CreateShaper)?;
        let mut actor = Self {
            surfaces: OpenSurfaces::new(preference_overrides, offered),
            outputs: Outputs::new(host, panels_per_output),
            actors,
            shaper,
            requests,
            input: InputRouter::new(input_filter),
            accessibility: offered.contains(&Capability::Accessibility),
            transfers: Transfers::new(clipboard, drags, watches, &action_queue, offered),
            action_queue,
            observers,
            run_end,
            pacing: FramePacing::default(),
            clock,
        };
        actor.open_setup_surfaces(surfaces, session_lock)?;
        Ok(actor)
    }

    pub(crate) fn reports(
        &self,
        inbox: &Sender<ViewInput>,
    ) -> Result<Reports<P::Control, P::Backend>, RendererStopped> {
        self.actors.reports(inbox, Arc::clone(&self.clock))
    }

    pub(crate) fn run(&mut self, inputs: &DeadlineReceiver<ViewInput>) -> Result<(), ViewError> {
        loop {
            self.turn()?;
            if self.finished() {
                return Ok(());
            }
            for request in self.transfers.watches.take_requests() {
                self.request(PlatformRequest::Watch(request))?;
            }
            match inputs.take_until(self.take_deadline())? {
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

    pub(crate) fn stop(
        mut self,
        inputs: &DeadlineReceiver<ViewInput>,
        ran: Result<(), ViewError>,
    ) -> (Result<(), ViewError>, ActorStops<P>) {
        let (renderer, controls, attached) = match &mut self.actors.attached {
            AttachedSurfaces::Rendered(renderer) => (renderer.stop(), Ok(()), Stopped::Renderer),
            AttachedSurfaces::Unrendered(controls) => (Ok(()), controls.stop(), Stopped::Controls),
        };
        let ran = ran.and_then(|()| self.handle_until(inputs, attached));
        let scheduler = self.actors.scheduler.stop();
        let ran = ran.and_then(|()| self.handle_until(inputs, Stopped::Scheduler));
        let scene = self.actors.scene.stop();
        self.action_queue.close();
        self.transfers.close(self.surfaces.records_mut());
        let stops = ActorStops {
            renderer,
            controls,
            scheduler,
            scene,
        };
        (ran, stops)
    }

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

    fn open_setup_surfaces(
        &mut self,
        surfaces: Vec<SurfaceSetup>,
        session_lock: Option<LockConfiguration>,
    ) -> Result<(), ViewError> {
        for surface in surfaces {
            match surface {
                SurfaceSetup::Window(window, view) => self.open_window(window, view)?,
                SurfaceSetup::Panel(options, view) => {
                    let _opened_panel_is_not_needed = self.open_panel(options, view, None)?;
                }
            }
        }
        if let Some(configuration) = session_lock {
            self.start_lock(configuration)?;
        }
        Ok(())
    }

    fn turn(&mut self) -> Result<(), ViewError> {
        self.update_panels()?;
        self.update_lock()?;
        self.render()?;
        for request in self.surfaces.focus_grab_requests() {
            self.request(PlatformRequest::FocusGrab(request))?;
        }
        self.send_window_requests()
    }

    fn send_window_requests(&self) -> Result<(), ViewError> {
        let requests = self.surfaces.records().take_window_requests();
        (requests.into_iter())
            .filter_map(|(surface, request)| {
                to_window_control_request(surface, request, self.input.last_press(surface))
            })
            .try_for_each(|control| self.request(PlatformRequest::WindowControl(control)))
    }

    fn handle(&mut self, input: ViewInput) -> Result<(), ViewError> {
        match input {
            ViewInput::Platform(event) => self.platform_event(event),
            ViewInput::Scheduler(output) => {
                self.scheduler_output(output);
                Ok(())
            }
            ViewInput::Renderer(output) => self.renderer_output(output),
            ViewInput::Controls(output) => self.controls_output(output),
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

    fn schedule(&self, input: SchedulerInput) {
        let _gone_scheduler_ends_the_run = self.actors.scheduler.send(input);
    }

    fn take_deadline(&mut self) -> Option<Duration> {
        let now = self.clock.now();
        let exit = (self.run_end.exit_in(now)).and_then(|wait| now.checked_add(wait));
        exit.into_iter().chain(self.pacing.take_deadline(now)).min()
    }

    fn finished(&mut self) -> bool {
        let anything_open = self.outputs.keeps_run_going() || self.surfaces.records().any_open();
        (self.run_end).finished(self.clock.now(), anything_open)
    }

    fn start_lock(&mut self, configuration: LockConfiguration) -> Result<(), ViewError> {
        let outputs = self.outputs.start_lock(configuration)?;
        self.request(PlatformRequest::Lock(SessionLockRequest::Lock))?;
        for output in outputs {
            self.open_lock_surface(output)?;
        }
        Ok(())
    }
}

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
