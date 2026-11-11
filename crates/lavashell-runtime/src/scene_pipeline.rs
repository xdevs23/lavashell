// SPDX-License-Identifier: GPL-3.0-only
use crate::builder_actor::BuilderActor;
use crate::choreographer_actor::{ChoreographerActor, ChoreographerInput};
use crate::scene_item::SceneItem;
use crate::scheduler::FrameSequence;
use crate::surface_slots::{BuiltSurface, SurfaceSlots};
use lavashell_actor::{Clock, Closed, Handle, Sender, mailbox_on, spawn};
use lavashell_core::error::message_chain;
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SinkId};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use lavashell_core::view::View;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

mod error;

pub use error::Error;

/// What the scene builder reports about the scene for a surface.
#[derive(Clone, Copy, Debug)]
pub enum BuilderReport {
    /// The scene contains every frame that the view composed up to the frame with `sequence`. A scene built from a
    /// frame sent outside a compose has no sequence.
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
    },
    /// A frame sent outside a compose changed the surface state and left the scene as it was. The host commits the
    /// built state without a frame.
    StateChanged { surface: SurfaceId },
    /// A frame did not fit the scene. The scene shows the last frame that it built, until the host passes the report
    /// to `ScenePipeline::restart_view`.
    Restart {
        surface: SurfaceId,
        error: ApplyError,
    },
}

/// Receives each report of the scene builder. The builder stops once it fails.
pub type BuilderObserver = Box<dyn FnMut(BuilderReport) -> Result<(), Closed> + Send>;

/// Runs the choreographer and the scene builder on separate threads between the views and the renderer. Each view
/// sends frames to the sink for the surface it shows. The choreographer merges the frames until the scene builder
/// requests them, and the builder publishes the scene and the surface state for each surface to the renderer.
pub struct ScenePipeline {
    choreographer: Sender<ChoreographerInput>,
    sinks: BTreeMap<SurfaceId, SinkState>,
    slots: Arc<SurfaceSlots>,
    run: Run,
}

/// The actors while they run, and how they ended once they stopped.
enum Run {
    Running(Handle<ChoreographerActor>, Handle<BuilderActor>),
    Ended(Result<(), Arc<Error>>),
}

/// The change sink that sends one frame of a surface to the choreographer. A frame contains the changes sent by the
/// view and the changes of the surface state. It ends with the frame of the view, or with
/// [`SceneSink::end_state_frame`].
pub struct SceneSink<'a> {
    surface: SurfaceId,
    /// The sequence of the frame that the view composes, or `None` for a frame sent outside a compose.
    end: Option<FrameSequence>,
    state: &'a mut SinkState,
    choreographer: &'a Sender<ChoreographerInput>,
}

/// What the sink for a surface keeps from one frame to the next.
#[derive(Default)]
struct SinkState {
    id: SinkId,
    any_frame_ended: bool,
    /// The surface state sent so far. A restart sends it again.
    surface: SurfaceState,
    open: OpenFrame,
}

/// What the open frame of a sink contains, apart from the changes sent by the view.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum OpenFrame {
    #[default]
    WithoutState,
    WithState,
    /// The sink restarted. The open frame contains the whole surface state and waits for the whole scene, sent with
    /// the next frame of the view.
    AwaitingScene,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    Ended,
    Open,
}

impl ScenePipeline {
    /// Starts the choreographer and the scene builder, whose mailboxes `clock` counts. The builder passes each report
    /// to `observer`.
    ///
    /// # Errors
    ///
    /// Fails when the thread of either actor cannot start.
    pub fn start(observer: BuilderObserver, clock: &dyn Clock) -> Result<Self, Error> {
        let (choreographer, choreographer_inbox) = mailbox_on(clock);
        let (builder, builder_inbox) = mailbox_on(clock);
        let intake = choreographer_inbox.intake();
        let choreographer_actor = spawn("choreographer", choreographer_inbox, move || {
            Ok(ChoreographerActor::new(builder, intake))
        })
        .map_err(Error::StartChoreographer)?;
        let slots = Arc::new(SurfaceSlots::default());
        let (built, requesting) = (Arc::clone(&slots), choreographer.clone());
        let spawned = spawn("scene builder", builder_inbox, move || {
            Ok(BuilderActor::new(built, requesting, observer))
        });
        let builder_actor = match spawned {
            Ok(builder_actor) => builder_actor,
            Err(error) => {
                drop(choreographer);
                let _choreographer_end = choreographer_actor.join();
                return Err(Error::StartBuilder(error));
            }
        };
        Ok(Self {
            choreographer,
            sinks: BTreeMap::new(),
            slots,
            run: Run::Running(choreographer_actor, builder_actor),
        })
    }

    /// The sink for the next frame of `surface`, which ends the frame with `end`.
    pub fn sink(&mut self, surface: SurfaceId, end: Option<FrameSequence>) -> SceneSink<'_> {
        SceneSink {
            surface,
            end,
            state: self.sinks.entry(surface).or_default(),
            choreographer: &self.choreographer,
        }
    }

    /// Reacts to a frame that did not fit the scene for `surface`, as reported by `BuilderReport::Restart`. Logs
    /// `error` as a warning and assigns a new id to the sink for `surface`. `view` then sends the whole scene with
    /// the next frame.
    pub fn restart_view(&mut self, surface: SurfaceId, view: &mut View, error: &ApplyError) {
        log::warn!(
            "a frame did not fit the built scene. The next frame sends the whole scene: {}",
            message_chain(error)
        );
        if let Some(sink) = self.sinks.get_mut(&surface) {
            sink.id = SinkId::default();
        }
        view.invalidate();
    }

    /// The newest scene and surface state that the scene builder built for `surface`.
    #[must_use]
    pub fn built(&self, surface: SurfaceId) -> Arc<BuiltSurface> {
        self.slots.read(surface)
    }

    /// Drops the sink and the scene for `surface` once the surface retired.
    ///
    /// # Errors
    ///
    /// Fails when the choreographer is gone.
    pub fn retire(&mut self, surface: SurfaceId) -> Result<(), Closed> {
        let _retired_sink = self.sinks.remove(&surface);
        self.choreographer.push(ChoreographerInput::Retire(surface))
    }

    /// Stops the choreographer and the scene builder, and waits until both threads end.
    ///
    /// # Errors
    ///
    /// Fails with the failure of either actor, on this call and on every later call.
    pub fn stop(&mut self) -> Result<(), Arc<Error>> {
        let ended = match mem::replace(&mut self.run, Run::Ended(Ok(()))) {
            Run::Running(choreographer, builder) => {
                let _choreographer_gone = self.choreographer.push(ChoreographerInput::Stop);
                let choreographer = choreographer.join().map_err(Error::RunChoreographer);
                let builder = builder.join().map_err(Error::RunBuilder);
                builder.and(choreographer).map_err(Arc::new)
            }
            Run::Ended(ended) => ended,
        };
        self.run = Run::Ended(ended.clone());
        ended
    }
}

impl Drop for ScenePipeline {
    fn drop(&mut self) {
        let _failure = self.stop();
    }
}

impl SceneSink<'_> {
    /// Whether any frame of the view ended into the sink for the surface. A restart keeps the built scene until the
    /// next frame ends, so it does not reset this.
    #[must_use]
    pub fn any_frame_ended(&self) -> bool {
        self.state.any_frame_ended
    }

    /// Sends `change` of the surface state with the open frame, unless the state sent before has the value of
    /// `change`.
    ///
    /// # Errors
    ///
    /// Fails when the choreographer is gone.
    pub fn set_state(&mut self, change: SurfaceChange) -> Result<(), Closed> {
        if self.state.surface.contains(&change) {
            return Ok(());
        }
        self.push(SceneItem::Surface(change.clone()))?;
        self.state.surface.apply(change);
        if self.state.open == OpenFrame::WithoutState {
            self.state.open = OpenFrame::WithState;
        }
        Ok(())
    }

    /// Ends the open frame unless it waits for the whole scene. A sink without a sequence ends only a frame that
    /// contains a change of the surface state.
    ///
    /// # Errors
    ///
    /// Fails when the choreographer is gone.
    pub fn end_state_frame(&mut self) -> Result<FrameStatus, Closed> {
        let status = match (self.state.open, self.end) {
            (OpenFrame::WithState, _) | (OpenFrame::WithoutState, Some(_)) => FrameStatus::Ended,
            (OpenFrame::WithoutState, None) | (OpenFrame::AwaitingScene, _) => FrameStatus::Open,
        };
        if status == FrameStatus::Ended {
            (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
            self.state.open = OpenFrame::WithoutState;
        }
        Ok(status)
    }

    fn push(&self, item: SceneItem) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Change(self.surface, Box::new(item)))
    }
}

impl ChangeSink for SceneSink<'_> {
    type Error = Closed;

    fn id(&self) -> SinkId {
        self.state.id
    }

    fn change(&mut self, change: Change) -> Result<(), Closed> {
        self.push(SceneItem::Change(change))
    }

    fn end_frame(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
        self.state.any_frame_ended = true;
        self.state.open = OpenFrame::WithoutState;
        Ok(())
    }

    /// Drops the frames that wait for the builder, and sends the whole surface state again, because the dropped
    /// frames can contain changes of it.
    fn restart(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Restart(self.surface))?;
        for change in self.state.surface.changes() {
            self.push(SceneItem::Surface(change))?;
        }
        self.state.open = OpenFrame::AwaitingScene;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
