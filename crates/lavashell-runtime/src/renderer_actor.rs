// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::CpuStamp;
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{
    Actor, Clock, Closed, Effect, Flow, Handle, JoinError, Merge, Outlet, Sender, SpawnError,
    mailbox_on, spawn,
};
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::SurfaceAttachment;
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use lavashell_renderer::vulkan::PresentationBackend;
use lavashell_renderer::{
    CommitError, DrawError, Drawn, PresentFrame, RendererSetup, RendererState, StateError,
    SurfaceContents, SurfaceControl,
};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// What the platform sends to the renderer.
pub enum RendererInput<C: SurfaceControl> {
    /// Attaches, controls or detaches a native surface. The renderer fails when a surface cannot be attached, and
    /// reports each detached surface with [`RendererOutput::Retired`] after the inputs sent before.
    Surface(SurfaceAttachment<C>),
    /// Sets `outline`, in physical pixels, to clip every later frame of `surface`. A later outline of the surface
    /// replaces an outline that the renderer has not set yet.
    Outline {
        surface: SurfaceId,
        outline: Option<Path>,
    },
    /// Draws the scene of `built` as the frame of `surface` for `time` in `viewport`, and applies the surface state
    /// of `built` with the frame. A later frame of the surface replaces a frame that the renderer has not drawn yet.
    Render {
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        built: Arc<BuiltSurface>,
    },
    /// Makes the host apply the surface state of `built` to `surface` without a frame. The renderer reports each
    /// commit with [`RendererOutput::Committed`].
    CommitState {
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    },
    /// Asks the scene source for the next image in `viewport` once the image is due.
    AdvanceScene(Viewport),
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

impl<C: SurfaceControl> Merge for RendererInput<C> {
    type Key = RendererKey;

    fn effect(&self) -> Effect<RendererKey> {
        match self {
            Self::Outline { surface, .. } => Effect::Overwrites(RendererKey::Outline(*surface)),
            Self::Render { surface, .. } => Effect::Overwrites(RendererKey::Render(*surface)),
            Self::AdvanceScene(_) => Effect::Overwrites(RendererKey::AdvanceScene),
            Self::Surface(_) | Self::CommitState { .. } | Self::Stop => Effect::BuildsOn,
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

/// Receives each output of the renderer. The renderer stops once it fails.
pub type RendererObserver = Box<dyn FnMut(RendererOutput) -> Result<(), Closed> + Send>;

/// The renderer stopped, so it handles no more inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the renderer stopped")]
pub struct RendererStopped;

/// A failure of the renderer, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum RendererError<E> {
    #[error("the renderer could not start")]
    Start(#[source] StateError),
    #[error("a surface could not be attached")]
    Attach(#[source] StateError),
    #[error("the outline of a surface could not be set")]
    Outline(#[source] StateError),
    #[error("a frame could not be drawn")]
    Draw(#[source] DrawError<E>),
    #[error("the contents of a drawn surface could not be checked")]
    ReadContents(#[source] StateError),
    #[error("the state of a surface could not be committed without a frame")]
    CommitState(#[source] CommitError<E>),
    #[error("the scene source could not advance")]
    AdvanceScene(#[source] StateError),
}

/// Runs the renderer on its own thread. The renderer draws the scenes sent by the platform, and runs the control of
/// each surface on the renderer thread. Dropping the renderer stops it and discards the failure that
/// [`Renderer::stop`] returns.
pub struct Renderer<C: SurfaceControl + Send + 'static> {
    running: Option<Running<C>>,
    text_rendering: TextRendering,
}

/// The inbox and the thread of a renderer that has not been stopped.
struct Running<C: SurfaceControl + Send + 'static> {
    inbox: Sender<RendererInput<C>>,
    actor: Handle<RendererActor<C>>,
}

impl<C: SurfaceControl + Send + 'static> Renderer<C> {
    /// Starts the renderer on a thread that builds the GPU renderer and the painter for the surfaces of `backend` from
    /// `setup`. The renderer measures the times passed to the scene source on `clock`, measures the CPU time of each
    /// draw when `cpu_timing` is set, and passes each output to `observer`.
    ///
    /// # Errors
    ///
    /// Fails when the thread cannot be created, or when the GPU renderer or the painter cannot be created.
    pub fn start<B: PresentationBackend + Send + 'static>(
        backend: B,
        setup: RendererSetup,
        clock: Arc<dyn Clock>,
        cpu_timing: bool,
        observer: RendererObserver,
    ) -> Result<Self, SpawnError<RendererError<C::Error>>> {
        let (inbox, receiver) = mailbox_on(clock.as_ref());
        let (rendering, started_rendering) = mpsc::sync_channel(1);
        let actor = spawn("renderer", receiver, move || {
            let state = RendererState::new(&backend, setup).map_err(RendererError::Start)?;
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
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        Ok(running.inbox.outlet(RendererInput::Surface))
    }

    /// Sends `input` to the renderer.
    ///
    /// # Errors
    ///
    /// Fails when the renderer stopped.
    pub fn send(&self, input: RendererInput<C>) -> Result<(), RendererStopped> {
        let running = self.running.as_ref().ok_or(RendererStopped)?;
        running.inbox.push(input).map_err(|Closed| RendererStopped)
    }

    /// Stops the renderer after the inputs sent before, also while a link from a platform to the renderer exists, and
    /// waits until the renderer thread has dropped every target and control.
    ///
    /// # Errors
    ///
    /// Fails with the failure of the renderer.
    pub fn stop(&mut self) -> Result<(), JoinError<RendererError<C::Error>>> {
        let Some(Running { inbox, actor }) = self.running.take() else {
            return Ok(());
        };
        let _renderer_gone = inbox.push(RendererInput::Stop);
        drop(inbox);
        actor.join()
    }
}

impl<C: SurfaceControl + Send + 'static> Drop for Renderer<C> {
    fn drop(&mut self) {
        let _failure = self.stop();
    }
}

/// The GPU renderer, the painter and the attached surfaces, on the renderer thread.
struct RendererActor<C: SurfaceControl> {
    state: RendererState<C>,
    /// The clock on which the scene source measures the time of each image.
    clock: Arc<dyn Clock>,
    cpu_timing: bool,
    observer: RendererObserver,
}

impl<C: SurfaceControl + Send + 'static> Actor for RendererActor<C> {
    type Input = RendererInput<C>;
    type Error = RendererError<C::Error>;

    fn handle(&mut self, input: RendererInput<C>) -> Result<Flow, RendererError<C::Error>> {
        let deadline = self.state.scene_deadline();
        let output = match input {
            RendererInput::Surface(attachment) => self.attachment(attachment)?,
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
            RendererInput::AdvanceScene(viewport) => {
                (self.state.advance_scene(viewport, self.clock.now()))
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

impl<C: SurfaceControl> RendererActor<C> {
    /// Attaches, controls or detaches a native surface, and returns the report of a detached surface.
    fn attachment(
        &mut self,
        attachment: SurfaceAttachment<C>,
    ) -> Result<Option<RendererOutput>, RendererError<C::Error>> {
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

    /// Draws the scene of `built` with the surface state built from the same frames.
    fn render(
        &mut self,
        surface: SurfaceId,
        time: FrameTime,
        viewport: Viewport,
        built: &BuiltSurface,
    ) -> Result<RendererOutput, RendererError<C::Error>> {
        let started = Instant::now();
        let cpu_start = self.cpu_timing.then(CpuStamp::now).flatten();
        let frame = PresentFrame {
            time,
            viewport,
            present_at: None,
            state: &built.state,
        };
        let drawn = (self.state.draw(surface, frame, &built.scene)).map_err(RendererError::Draw)?;
        let cost = started.elapsed();
        let cpu_time = cpu_start.and_then(CpuStamp::elapsed);
        let contents =
            (self.state.contents(surface, viewport)).map_err(RendererError::ReadContents)?;
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

/// The report of the scene deadline `after` an input that found the deadline `before`, or `None` when the deadline did
/// not change. A scene source that produced nothing keeps a deadline that has passed, which the platform knows already.
fn scene_wake(before: Option<Duration>, after: Option<Duration>) -> Option<RendererOutput> {
    (after != before).then_some(RendererOutput::SceneWake(after))
}

#[cfg(test)]
mod tests;
