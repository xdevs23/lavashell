// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::CpuStamp;
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{
    Actor, Clock, Closed, Effect, Flow, Handle, JoinError, Merge, Outlet, Sender, SpawnError,
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
use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// What the platform sends to the renderer. `S` is the streams type.
pub enum RendererInput<C: SurfaceControl, S> {
    /// Attaches, controls or detaches a native surface. The renderer stops with an error when a surface cannot be
    /// attached. It sends [`RendererOutput::Retired`] for each detached surface, after processing all earlier inputs.
    Surface(SurfaceAttachment<C>),
    /// Runs the task with the streams, after processing all earlier inputs.
    Streams(StreamTask<S>),
    /// Sets `outline`, in dp, to clip every later frame of `surface`. A later outline of the surface
    /// replaces an outline that the renderer has not set yet.
    Outline {
        surface: SurfaceId,
        outline: Option<Path>,
    },
    /// Draws the scene of `built` as the frame of `surface` for `time` in `viewport` at `scale`, and applies the
    /// surface state of `built` with the frame. A later frame of the surface replaces a frame that the renderer has
    /// not drawn yet.
    Render {
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        scale: PixelScale,
        built: Arc<BuiltSurface>,
    },
    /// Makes the host apply the surface state of `built` to `surface` without a frame. The renderer reports each
    /// commit with [`RendererOutput::Committed`].
    CommitState {
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    },
    /// Asks the scene source for the next image in `viewport` at `scale` once the image is due.
    AdvanceScene {
        viewport: Viewport,
        scale: PixelScale,
    },
    /// Ends the renderer after the inputs sent before, also while other senders to the inbox exist.
    Stop,
}

/// The value that an input to the renderer overwrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RendererKey {
    Outline(SurfaceId),
    Render(SurfaceId),
    AdvanceScene,
}

impl<C: SurfaceControl, S> Merge for RendererInput<C, S> {
    type Key = RendererKey;

    fn effect(&self) -> Effect<RendererKey> {
        match self {
            Self::Outline { surface, .. } => Effect::Overwrites(RendererKey::Outline(*surface)),
            Self::Render { surface, .. } => Effect::Overwrites(RendererKey::Render(*surface)),
            Self::AdvanceScene { .. } => Effect::Overwrites(RendererKey::AdvanceScene),
            Self::Surface(_) | Self::Streams(_) | Self::CommitState { .. } | Self::Stop => {
                Effect::BuildsOn
            }
        }
    }
}

/// What the renderer reports to the platform.
pub enum RendererOutput {
    Rendered(Box<Rendered>),
    /// The control of the surface sent the surface state of a [`RendererInput::CommitState`] and the commit to the
    /// host.
    Committed(SurfaceId),
    /// The renderer dropped the target and the control of the surface, so nothing presents to the surface anymore.
    Retired(SurfaceId),
    /// The time on the run clock at which the scene source produces the next image, or `None` while the source cannot
    /// submit another image. The renderer sends the time each time it changes.
    SceneWake(Option<Duration>),
}

/// A frame drawn by the renderer.
pub struct Rendered {
    pub surface: SurfaceId,
    pub time: FrameTime,
    pub viewport: Viewport,
    /// The number of output pixels covered by one viewport pixel of the frame.
    pub scale: PixelScale,
    pub drawn: Drawn,
    /// Whether the target of the surface contains the drawn frame after the draw.
    pub contents: SurfaceContents,
    /// The time after the presentation of the frame at which the drawn scene changes by itself, or `None` when it
    /// changes only with a new frame of the view.
    pub next_change: Option<Duration>,
    /// The wall time that the draw took.
    pub cost: Duration,
    /// The CPU time that the renderer thread spent on the draw, or `None` while the renderer does not measure it.
    pub cpu_time: Option<Duration>,
}

/// The error of the renderer of a run on `P`.
pub type PlatformRendererError<P> =
    RendererError<<P as Platform>::Control, <P as Platform>::Streams>;

/// Receives each output of the renderer. The renderer stops once it fails.
pub type RendererObserver = Box<dyn FnMut(RendererOutput) -> Result<(), Closed> + Send>;

/// The renderer stopped, so it handles no more inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the renderer stopped")]
pub struct RendererStopped;

/// A failure of the renderer, named after the failing operation. `C` is the type of the native-surface controls. `S`
/// is the streams type.
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

/// Runs the renderer on its own thread. The renderer draws the scenes sent by the platform, and runs the control of
/// each surface and the streams on the renderer thread. Dropping the renderer stops it and discards the failure that
/// [`Renderer::stop`] returns.
pub struct Renderer<C, S>
where
    C: SurfaceControl + Send + 'static,
    S: RendererStreams,
{
    running: Option<Running<C, S>>,
    text_rendering: TextRendering,
}

/// The inbox and the thread of a renderer that has not been stopped.
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
    /// Starts the renderer on a thread that builds the GPU renderer and the painter for the surfaces of `backend` from
    /// `setup`, and the streams from `streams`. The renderer measures the times passed to the scene source on
    /// `clock`, measures the CPU time of each draw when `cpu_timing` is set, and passes each output to `observer`.
    ///
    /// # Errors
    ///
    /// Fails when the thread cannot be created, or when the GPU renderer, the painter or the streams cannot be
    /// created.
    pub fn start<B: PresentationBackend + Send + 'static>(
        backend: B,
        setup: RendererSetup,
        streams: StreamSetup<S>,
        clock: Arc<dyn Clock>,
        cpu_timing: bool,
        observer: RendererObserver,
    ) -> Result<Self, SpawnError<RendererError<C, S>>> {
        let (inbox, receiver) = mailbox_on(clock.as_ref());
        let (rendering, started_rendering) = mpsc::sync_channel(1);
        let actor = spawn("renderer", receiver, move || {
            let state = RendererState::with_streams(&backend, setup, streams)
                .map_err(RendererError::Start)?;
            let _sent = rendering.send(state.text_rendering());
            Ok(RendererActor {
                state,
                clock,
                cpu_timing,
                observer,
            })
        })?;
        // The start closure sent the text rendering before `spawn` returned.
        let text_rendering = started_rendering
            .recv()
            .map_err(|_ended| SpawnError::Ended)?;
        Ok(Self {
            running: Some(Running { inbox, actor }),
            text_rendering,
        })
    }

    /// The text rendering used by the painter on the renderer thread. A shaper on another thread follows it to
    /// measure text as the painter shapes it.
    #[must_use]
    pub fn text_rendering(&self) -> TextRendering {
        self.text_rendering
    }

    /// The outlet through which a platform attaches the native surfaces, passes messages to their controls and
    /// detaches them. The renderer retires a detached surface after the inputs sent before.
    pub(crate) fn attachments(&self) -> Result<Outlet<SurfaceAttachment<C>>, RendererStopped> {
        self.outlet(RendererInput::Surface)
    }

    /// The outlet through which a platform runs tasks with the streams, in order with the attachments.
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

    /// Sends `input` to the renderer.
    ///
    /// # Errors
    ///
    /// Fails when the renderer stopped.
    pub fn send(&self, input: RendererInput<C, S>) -> Result<(), RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        running.inbox.push(input).map_err(|Closed| RendererStopped)
    }

    /// Stops the renderer after the inputs sent before, also while a link from a platform to the renderer exists, and
    /// waits until the renderer thread has dropped every target and control.
    ///
    /// # Errors
    ///
    /// Fails with the failure of the renderer.
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
        let _failure = self.stop();
    }
}

/// The GPU renderer, the painter, the attached surfaces and the streams, on the renderer thread.
struct RendererActor<C: SurfaceControl, S> {
    state: RendererState<C, S>,
    /// The clock on which the scene source measures the time of each image.
    clock: Arc<dyn Clock>,
    cpu_timing: bool,
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
                scale,
                built,
            } => Some(self.render(surface, time, viewport, scale, &built)?),
            RendererInput::CommitState { surface, built } => {
                (self.state.commit_state(surface, &built.state))
                    .map_err(RendererError::CommitState)?;
                Some(RendererOutput::Committed(surface))
            }
            RendererInput::AdvanceScene { viewport, scale } => {
                (self.state.advance_scene(viewport, scale, self.clock.now()))
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

impl<C, S> RendererActor<C, S>
where
    C: SurfaceControl,
    S: RendererStreams,
{
    /// Attaches, controls or detaches a native surface, and returns the report of a detached surface.
    fn attachment(
        &mut self,
        attachment: SurfaceAttachment<C>,
    ) -> Result<Option<RendererOutput>, RendererError<C, S>> {
        match attachment {
            SurfaceAttachment::Attach(attach) => {
                (self.state.attach(*attach))
                    .map_err(|failed| RendererError::Attach(failed.error))?;
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

    /// Draws the scene of `built` at `scale` with the surface state built from the same frames.
    fn render(
        &mut self,
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        scale: PixelScale,
        built: &BuiltSurface,
    ) -> Result<RendererOutput, RendererError<C, S>> {
        let started = Instant::now();
        let cpu_start = self.cpu_timing.then(CpuStamp::now).flatten();
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
            scale,
            drawn,
            contents,
            next_change: built.scene.next_change(time.presentation),
            cost,
            cpu_time,
        })))
    }
}

/// The report of the scene deadline `after` an input that found the deadline `before`, or `None` when the deadline did
/// not change. A scene source that produced nothing keeps a deadline that has passed, which the platform knows already.
fn scene_wake(before: Option<Duration>, after: Option<Duration>) -> Option<RendererOutput> {
    (after != before).then_some(RendererOutput::SceneWake(after))
}

#[cfg(test)]
mod tests;
