// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::{CpuStamp, CpuTiming};
use crate::failures::log_failure_to_stop;
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{
    Actor, Clock, Closed, Flow, Handle, JoinError, Merge, MergeEffect, Outlet, Sender, SpawnError,
    mailbox_on, spawn,
};
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::{PixelScale, Viewport};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Platform, StreamSetup, StreamTask, SurfaceAttachment};
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use lavashell_renderer::vulkan::PresentationBackend;
use lavashell_renderer::{
    CommitError, DrawError, Drawn, PresentFrame, RendererSetup, RendererState, RendererStreams,
    StartError, StateError, StreamBackend, SurfaceContents, SurfaceControl,
};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub enum RendererInput<C: SurfaceControl, S> {
    Surface(SurfaceAttachment<C>),
    Streams(StreamTask<S>),
    Outline {
        surface: SurfaceId,
        outline: Option<Path>,
    },
    Render {
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        built: Arc<BuiltSurface>,
    },
    CommitState {
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    },
    AdvanceScene {
        surface: SurfaceId,
        viewport: Viewport,
    },
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RendererKey {
    Outline(SurfaceId),
    Render(SurfaceId),
    AdvanceScene,
}

impl<C: SurfaceControl, S> Merge for RendererInput<C, S> {
    type Key = RendererKey;

    fn effect(&self) -> MergeEffect<RendererKey> {
        match self {
            Self::Outline { surface, .. } => {
                MergeEffect::Overwrites(RendererKey::Outline(*surface))
            }
            Self::Render { surface, .. } => MergeEffect::Overwrites(RendererKey::Render(*surface)),
            Self::AdvanceScene { .. } => MergeEffect::Overwrites(RendererKey::AdvanceScene),
            Self::Surface(_) | Self::Streams(_) | Self::CommitState { .. } | Self::Stop => {
                MergeEffect::Appends
            }
        }
    }
}

pub enum RendererOutput {
    Rendered(Box<Rendered>),
    Committed(SurfaceId),
    Retired(SurfaceId),
    SceneWake(Option<Duration>),
}

pub struct Rendered {
    pub surface: SurfaceId,
    pub time: FrameTime,
    pub viewport: Viewport,
    pub drawn: Drawn,
    pub contents: SurfaceContents,
    pub next_change: Option<Duration>,
    pub cost: Duration,
    pub cpu_time: Option<Duration>,
}

pub type PlatformRendererError<P> =
    RendererError<<P as Platform>::Control, <P as Platform>::Streams>;

pub type RendererObserver = Box<dyn FnMut(RendererOutput) -> Result<(), Closed> + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the renderer stopped")]
pub struct RendererStopped;

#[derive(thiserror::Error)]
pub enum RendererError<C: SurfaceControl, S: StreamBackend> {
    #[error("the renderer could not start")]
    Start(#[source] StartError<S::Error>),
    #[error("a surface could not be attached")]
    Attach(#[source] StateError),
    #[error("the outline of a surface could not be set")]
    Outline(#[source] StateError),
    #[error("a frame could not be drawn")]
    Draw(#[source] DrawError<C::Error, S::Error>),
    #[error("the contents of a drawn surface could not be checked")]
    ReadContents(#[source] StateError),
    #[error("the state of a surface could not be committed without a frame")]
    CommitState(#[source] CommitError<C::Error>),
    #[error("the scene source could not advance")]
    AdvanceScene(#[source] StateError),
}

impl<C: SurfaceControl, S: StreamBackend> fmt::Debug for RendererError<C, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, source): (&str, &dyn fmt::Debug) = match self {
            Self::Start(error) => ("Start", error),
            Self::Attach(error) => ("Attach", error),
            Self::Outline(error) => ("Outline", error),
            Self::Draw(error) => ("Draw", error),
            Self::ReadContents(error) => ("ReadContents", error),
            Self::CommitState(error) => ("CommitState", error),
            Self::AdvanceScene(error) => ("AdvanceScene", error),
        };
        formatter.debug_tuple(name).field(source).finish()
    }
}

pub struct Renderer<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    running: Option<Running<C, S>>,
    text_rendering: TextRendering,
}

struct Running<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    inbox: Sender<RendererInput<C, S>>,
    actor: Handle<RendererActor<C, S>>,
}

impl<C, S> Renderer<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    pub fn start<B: PresentationBackend + Send + 'static>(
        backend: B,
        setup: RendererSetup,
        streams: StreamSetup<S>,
        clock: Arc<dyn Clock>,
        cpu_timing: CpuTiming,
        observer: RendererObserver,
    ) -> Result<Self, SpawnError<RendererError<C, S>>> {
        let (inbox, receiver) = mailbox_on(clock.as_ref());
        let (rendering, started_rendering) = mpsc::sync_channel(1);
        let actor = spawn("renderer", receiver, move || {
            let state = RendererState::with_streams(&backend, setup, streams)
                .map_err(RendererError::Start)?;
            let _spawner_may_have_left = rendering.send(state.text_rendering());
            Ok(RendererActor {
                state,
                scales: BTreeMap::new(),
                clock,
                cpu_timing,
                observer,
            })
        })?;
        let text_rendering = started_rendering
            .recv()
            .map_err(|mpsc::RecvError| SpawnError::Panicked)?;
        Ok(Self {
            running: Some(Running { inbox, actor }),
            text_rendering,
        })
    }

    #[must_use]
    pub fn text_rendering(&self) -> TextRendering {
        self.text_rendering
    }

    pub(crate) fn attachments(&self) -> Result<Outlet<SurfaceAttachment<C>>, RendererStopped> {
        self.outlet(RendererInput::Surface)
    }

    pub(crate) fn stream_tasks(&self) -> Result<Outlet<StreamTask<S>>, RendererStopped> {
        self.outlet(RendererInput::Streams)
    }

    fn outlet<U: 'static>(
        &self,
        convert: fn(U) -> RendererInput<C, S>,
    ) -> Result<Outlet<U>, RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        Ok(running.inbox.outlet(convert))
    }

    pub fn send(&self, input: RendererInput<C, S>) -> Result<(), RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        running.inbox.push(input).map_err(|Closed| RendererStopped)
    }

    pub fn stop(&mut self) -> Result<(), JoinError<RendererError<C, S>>> {
        let Some(Running { inbox, actor }) = self.running.take() else {
            return Ok(());
        };
        let _renderer_gone = inbox.push(RendererInput::Stop);
        drop(inbox);
        actor.join()
    }
}

impl<C, S> Drop for Renderer<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    fn drop(&mut self) {
        log_failure_to_stop(self.stop());
    }
}

struct RendererActor<C: SurfaceControl, S> {
    state: RendererState<C, S>,
    scales: BTreeMap<SurfaceId, PixelScale>,
    clock: Arc<dyn Clock>,
    cpu_timing: CpuTiming,
    observer: RendererObserver,
}

impl<C, S> Actor for RendererActor<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    type Input = RendererInput<C, S>;
    type Error = RendererError<C, S>;

    fn handle(&mut self, input: RendererInput<C, S>) -> Result<Flow, RendererError<C, S>> {
        let deadline = self.state.scene_deadline();
        let output = match input {
            RendererInput::Surface(attachment) => self.attachment(attachment)?,
            RendererInput::Streams(task) => {
                self.state.run_on_streams(task);
                None
            }
            RendererInput::Outline { surface, outline } => {
                (self.state.set_outline(surface, outline)).map_err(RendererError::Outline)?;
                None
            }
            RendererInput::Render {
                surface,
                time,
                viewport,
                built,
            } => Some(self.render(surface, time, viewport, &built)?),
            RendererInput::CommitState { surface, built } => {
                (self.state.commit_state(surface, &built.state))
                    .map_err(RendererError::CommitState)?;
                Some(RendererOutput::Committed(surface))
            }
            RendererInput::AdvanceScene { surface, viewport } => {
                if let Some(&scale) = self.scales.get(&surface) {
                    (self.state.advance_scene(viewport, scale, self.clock.now()))
                        .map_err(RendererError::AdvanceScene)?;
                }
                None
            }
            RendererInput::Stop => return Ok(Flow::Stop),
        };
        let wake = scene_wake(deadline, self.state.scene_deadline());
        let sent = (output.into_iter().chain(wake)).try_for_each(&mut self.observer);
        Ok(sent.map_or(Flow::Stop, |()| Flow::Continue))
    }
}

impl<C, S> RendererActor<C, S>
where
    C: SurfaceControl,
    S: RendererStreams,
{
    fn attachment(
        &mut self,
        attachment: SurfaceAttachment<C>,
    ) -> Result<Option<RendererOutput>, RendererError<C, S>> {
        match attachment {
            SurfaceAttachment::Attach { attach, scale } => {
                let surface = attach.surface;
                (self.state.attach(*attach))
                    .map_err(|failed| RendererError::Attach(failed.error))?;
                let _replaced_scale = self.scales.insert(surface, scale);
                Ok(None)
            }
            SurfaceAttachment::Control { surface, message } => {
                if let Some(control) = self.state.control_mut(surface) {
                    control.receive(message);
                }
                Ok(None)
            }
            SurfaceAttachment::Detach(surface) => {
                drop(self.state.retire(surface));
                let _retired_scale = self.scales.remove(&surface);
                Ok(Some(RendererOutput::Retired(surface)))
            }
        }
    }

    fn render(
        &mut self,
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        built: &BuiltSurface,
    ) -> Result<RendererOutput, RendererError<C, S>> {
        let started = Instant::now();
        let cpu_start = self.cpu_timing.stamp();
        let unknown = || RendererError::Draw(StateError::UnknownSurface(surface).into());
        let scale = *self.scales.get(&surface).ok_or_else(unknown)?;
        let frame = PresentFrame {
            time,
            viewport,
            scale,
            present_at: None,
            state: &built.state,
        };
        let drawn = (self.state.draw(surface, frame, &built.scene)).map_err(RendererError::Draw)?;
        let cost = started.elapsed();
        let cpu_time = cpu_start.and_then(CpuStamp::elapsed);
        let contents =
            (self.state.contents(surface, viewport, scale)).map_err(RendererError::ReadContents)?;
        Ok(RendererOutput::Rendered(Box::new(Rendered {
            surface,
            time,
            viewport,
            drawn,
            contents,
            next_change: built.scene.next_change(time.presentation),
            cost,
            cpu_time,
        })))
    }
}

fn scene_wake(before: Option<Duration>, after: Option<Duration>) -> Option<RendererOutput> {
    (after != before).then_some(RendererOutput::SceneWake(after))
}

#[cfg(test)]
mod tests;
