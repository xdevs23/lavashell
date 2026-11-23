// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::{CpuStamp, CpuTiming};
use crate::failures::log_failure_to_stop;
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{
    Actor, Clock, Closed, Flow, Handle, JoinError, Merge, MergeEffect, Outlet, Sender, SpawnError,
    mailbox_on, spawn,
};
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::ScaledViewport;
use lavashell_core::surface::SurfaceId;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::{
    Backend, Drawn, Platform, PresentFrame, RendererBackend, RendererSetup, StreamTask,
    SurfaceAttachment, SurfaceContents, SurfaceControl, SurfaceRenderer,
};
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub enum RendererInput<C: SurfaceControl, B: RendererBackend> {
    Surface(SurfaceAttachment<C, B::Surface>),
    Streams(StreamTask<B::Streams>),
    Outline {
        surface: SurfaceId,
        outline: Option<Path>,
    },
    Render {
        surface: SurfaceId,
        time: FrameTime,
        scaled_viewport: ScaledViewport,
        built: Arc<BuiltSurface>,
    },
    CommitState {
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    },
    AdvanceScene(ScaledViewport),
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RendererKey {
    Outline(SurfaceId),
    Render(SurfaceId),
    AdvanceScene,
}

impl<C: SurfaceControl, B: RendererBackend> Merge for RendererInput<C, B> {
    type Key = RendererKey;

    fn effect(&self) -> MergeEffect<RendererKey> {
        match self {
            Self::Outline { surface, .. } => {
                MergeEffect::Overwrites(RendererKey::Outline(*surface))
            }
            Self::Render { surface, .. } => MergeEffect::Overwrites(RendererKey::Render(*surface)),
            Self::AdvanceScene(_) => MergeEffect::Overwrites(RendererKey::AdvanceScene),
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
    pub scaled_viewport: ScaledViewport,
    pub drawn: Drawn,
    pub contents: SurfaceContents,
    pub next_change: Option<Duration>,
    pub cost: Duration,
    pub cpu_time: Option<Duration>,
}

pub type PlatformRendererError<P> =
    RendererError<<P as Platform>::Control, <P as Platform>::Backend>;

type StartedRenderer<C, B> = <B as RendererBackend>::Renderer<C>;

type SurfaceError<C, B> = <StartedRenderer<C, B> as SurfaceRenderer<
    C,
    <B as Backend>::Surface,
    <B as Backend>::Streams,
>>::Error;

type SurfaceDrawError<C, B> = <StartedRenderer<C, B> as SurfaceRenderer<
    C,
    <B as Backend>::Surface,
    <B as Backend>::Streams,
>>::DrawError;

type SurfaceCommitError<C, B> = <StartedRenderer<C, B> as SurfaceRenderer<
    C,
    <B as Backend>::Surface,
    <B as Backend>::Streams,
>>::CommitError;

pub type RendererObserver = Box<dyn FnMut(RendererOutput) -> Result<(), Closed> + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the renderer stopped")]
pub struct RendererStopped;

#[derive(thiserror::Error)]
pub enum RendererError<C: SurfaceControl, B: RendererBackend> {
    #[error("the renderer could not start")]
    Start(#[source] B::StartError),
    #[error("a surface could not be attached")]
    Attach(#[source] SurfaceError<C, B>),
    #[error("the outline of a surface could not be set")]
    Outline(#[source] SurfaceError<C, B>),
    #[error("a frame could not be drawn")]
    Draw(#[source] SurfaceDrawError<C, B>),
    #[error("the contents of a drawn surface could not be checked")]
    ReadContents(#[source] SurfaceError<C, B>),
    #[error("the state of a surface could not be committed without a frame")]
    CommitState(#[source] SurfaceCommitError<C, B>),
    #[error("the scene source could not advance")]
    AdvanceScene(#[source] SurfaceError<C, B>),
}

impl<C: SurfaceControl, B: RendererBackend> fmt::Debug for RendererError<C, B> {
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

impl<C: SurfaceControl, B: RendererBackend> NamesNextStep for RendererError<C, B>
where
    B::StartError: NamesNextStep,
    SurfaceError<C, B>: NamesNextStep,
    SurfaceDrawError<C, B>: NamesNextStep,
    SurfaceCommitError<C, B>: NamesNextStep,
{
    fn next_step(&self) -> NextStep {
        match self {
            Self::Start(error) => error.next_step(),
            Self::Attach(error)
            | Self::Outline(error)
            | Self::ReadContents(error)
            | Self::AdvanceScene(error) => error.next_step(),
            Self::Draw(error) => error.next_step(),
            Self::CommitState(error) => error.next_step(),
        }
    }
}

pub struct Renderer<C, B>
where
    C: SurfaceControl + Send + 'static,
    B: RendererBackend,
{
    running: Option<Running<C, B>>,
    text_rendering: TextRendering,
}

struct Running<C, B>
where
    C: SurfaceControl + Send + 'static,
    B: RendererBackend,
{
    inbox: Sender<RendererInput<C, B>>,
    actor: Handle<RendererActor<C, B>>,
}

impl<C, B> Renderer<C, B>
where
    C: SurfaceControl + Send + 'static,
    B: RendererBackend,
{
    pub fn start(
        backend: B,
        setup: RendererSetup,
        clock: Arc<dyn Clock>,
        cpu_timing: CpuTiming,
        observer: RendererObserver,
    ) -> Result<Self, SpawnError<RendererError<C, B>>> {
        let (inbox, receiver) = mailbox_on(clock.as_ref());
        let (rendering, started_rendering) = mpsc::sync_channel(1);
        let actor = spawn("renderer", receiver, move || {
            let state = backend.start(setup).map_err(RendererError::Start)?;
            let _spawner_may_have_left = rendering.send(state.text_rendering());
            Ok(RendererActor {
                state,
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

    pub(crate) fn attachments(
        &self,
    ) -> Result<Outlet<SurfaceAttachment<C, B::Surface>>, RendererStopped> {
        self.outlet(RendererInput::Surface)
    }

    pub(crate) fn stream_tasks(&self) -> Result<Outlet<StreamTask<B::Streams>>, RendererStopped> {
        self.outlet(RendererInput::Streams)
    }

    fn outlet<U: 'static>(
        &self,
        convert: fn(U) -> RendererInput<C, B>,
    ) -> Result<Outlet<U>, RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        Ok(running.inbox.outlet(convert))
    }

    pub fn send(&self, input: RendererInput<C, B>) -> Result<(), RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        running.inbox.push(input).map_err(|Closed| RendererStopped)
    }

    pub fn stop(&mut self) -> Result<(), JoinError<RendererError<C, B>>> {
        let Some(Running { inbox, actor }) = self.running.take() else {
            return Ok(());
        };
        let _renderer_gone = inbox.push(RendererInput::Stop);
        drop(inbox);
        actor.join()
    }
}

impl<C, B> Drop for Renderer<C, B>
where
    C: SurfaceControl + Send + 'static,
    B: RendererBackend,
{
    fn drop(&mut self) {
        log_failure_to_stop(self.stop());
    }
}

struct RendererActor<C: SurfaceControl, B: RendererBackend> {
    state: StartedRenderer<C, B>,
    clock: Arc<dyn Clock>,
    cpu_timing: CpuTiming,
    observer: RendererObserver,
}

impl<C, B> Actor for RendererActor<C, B>
where
    C: SurfaceControl + Send + 'static,
    B: RendererBackend,
{
    type Input = RendererInput<C, B>;
    type Error = RendererError<C, B>;

    fn handle(&mut self, input: RendererInput<C, B>) -> Result<Flow, RendererError<C, B>> {
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
                scaled_viewport,
                built,
            } => Some(self.render(surface, time, scaled_viewport, &built)?),
            RendererInput::CommitState { surface, built } => {
                (self.state.commit_state(surface, &built.state))
                    .map_err(RendererError::CommitState)?;
                Some(RendererOutput::Committed(surface))
            }
            RendererInput::AdvanceScene(scaled_viewport) => {
                (self.state.advance_scene(scaled_viewport, self.clock.now()))
                    .map_err(RendererError::AdvanceScene)?;
                None
            }
            RendererInput::Stop => return Ok(Flow::Stop),
        };
        let wake = scene_wake(deadline, self.state.scene_deadline());
        let sent = (output.into_iter().chain(wake)).try_for_each(&mut self.observer);
        Ok(sent.map_or(Flow::Stop, |()| Flow::Continue))
    }
}

impl<C, B> RendererActor<C, B>
where
    C: SurfaceControl,
    B: RendererBackend,
{
    fn attachment(
        &mut self,
        attachment: SurfaceAttachment<C, B::Surface>,
    ) -> Result<Option<RendererOutput>, RendererError<C, B>> {
        match attachment {
            SurfaceAttachment::Attach(attach) => {
                (self.state.attach(*attach)).map_err(RendererError::Attach)?;
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
                Ok(Some(RendererOutput::Retired(surface)))
            }
        }
    }

    fn render(
        &mut self,
        surface: SurfaceId,
        time: FrameTime,
        scaled_viewport: ScaledViewport,
        built: &BuiltSurface,
    ) -> Result<RendererOutput, RendererError<C, B>> {
        let started = Instant::now();
        let cpu_start = self.cpu_timing.stamp();
        let frame = PresentFrame {
            time,
            scaled_viewport,
            present_at: None,
            state: &built.state,
        };
        let drawn = (self.state.draw(surface, frame, &built.scene)).map_err(RendererError::Draw)?;
        let cost = started.elapsed();
        let cpu_time = cpu_start.and_then(CpuStamp::elapsed);
        let contents =
            (self.state.contents(surface, scaled_viewport)).map_err(RendererError::ReadContents)?;
        Ok(RendererOutput::Rendered(Box::new(Rendered {
            surface,
            time,
            scaled_viewport,
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
