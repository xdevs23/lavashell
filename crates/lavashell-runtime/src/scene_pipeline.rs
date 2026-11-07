// SPDX-License-Identifier: GPL-3.0-only
use crate::builder_actor::BuilderActor;
use crate::choreographer_actor::{ChoreographerActor, ChoreographerInput};
use crate::scene_item::SceneItem;
use crate::surface_slots::SurfaceSlots;
use crate::{Error, FrameSequence};
use lavashell_actor::{Closed, Handle, Sender, mailbox, spawn};
use lavashell_core::scene::tree::{ApplyError, Change, SceneSnapshot};
use lavashell_core::scene::{ChangeSink, SinkId};
use lavashell_core::surface::{SurfaceChange, SurfaceId, SurfaceState};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What the scene builder reports about the scene for a surface.
#[derive(Clone, Copy, Debug)]
pub enum BuilderReport {
    /// The scene and the surface state contain every frame that the view composed up to the frame with `sequence`. A
    /// scene built from a frame sent outside a compose has no sequence. `change` is what the built frames changed.
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
        change: BuiltChange,
    },
    /// A frame did not fit the scene. The scene shows the last frame that it built, until the view sends the whole
    /// scene to a restarted sink.
    Restart {
        surface: SurfaceId,
        error: ApplyError,
    },
}

/// What the frames built for a surface changed in the built scene of that surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltChange {
    Nothing,
    /// The surface state changed, and the snapshot is the one built before.
    State,
    /// The frames ended in the scene builder with a new snapshot. The surface state can have changed too.
    Scene,
}

/// The newest scene built for a surface, with the surface state built from the same frames.
pub struct BuiltScene {
    pub snapshot: SceneSnapshot,
    pub state: SurfaceState,
}

/// Receives each report of the scene builder. The builder stops once it fails.
pub type BuilderObserver = Box<dyn FnMut(BuilderReport) -> Result<(), Closed> + Send>;

/// Runs the choreographer and the scene builder on separate threads between the views and the renderer. Each view
/// sends frames to the sink for the surface it shows. The choreographer merges the frames until the scene builder
/// requests them, and the builder publishes the scene for each surface to the renderer.
pub struct ScenePipeline {
    choreographer: Sender<ChoreographerInput>,
    sinks: BTreeMap<SurfaceId, SinkState>,
    slots: Arc<SurfaceSlots>,
    actors: Option<(Handle<ChoreographerActor>, Handle<BuilderActor>)>,
}

/// The change sink that sends one frame for a surface to the choreographer. A frame contains the changes sent by the
/// view and the changes of the surface state. It ends with the frame of the view, or with
/// [`SceneSink::end_state_frame`].
pub struct SceneSink<'a> {
    surface: SurfaceId,
    /// The sequence of the frame that the view composes, or `None` for a frame sent outside a compose.
    end: Option<FrameSequence>,
    state: &'a mut SinkState,
    choreographer: &'a Sender<ChoreographerInput>,
}

/// The record of the sink for a surface across frames.
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

impl ScenePipeline {
    /// Starts the choreographer and the scene builder. The builder passes each report to `observer`.
    ///
    /// # Errors
    ///
    /// Fails when the thread of either actor cannot start.
    pub fn start(observer: BuilderObserver) -> Result<Self, Error> {
        let (choreographer, choreographer_inbox) = mailbox();
        let (builder, builder_inbox) = mailbox();
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
            actors: Some((choreographer_actor, builder_actor)),
        })
    }

    /// The sink for the next frame for `surface`, which ends the frame with `end`.
    pub fn sink(&mut self, surface: SurfaceId, end: Option<FrameSequence>) -> SceneSink<'_> {
        SceneSink {
            surface,
            end,
            state: self.sinks.entry(surface).or_default(),
            choreographer: &self.choreographer,
        }
    }

    /// Assigns a new id to the sink for `surface`, so the view sends the whole scene with the next frame.
    pub fn restart(&mut self, surface: SurfaceId) {
        if let Some(sink) = self.sinks.get_mut(&surface) {
            sink.id = SinkId::default();
        }
    }

    /// The newest scene that the scene builder built for `surface`, with the surface state built from the same frames.
    #[must_use]
    pub fn built(&self, surface: SurfaceId) -> Arc<BuiltScene> {
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
    /// Fails with the failure of either actor.
    pub fn stop(&mut self) -> Result<(), Error> {
        let Some((choreographer, builder)) = self.actors.take() else {
            return Ok(());
        };
        let _choreographer_gone = self.choreographer.push(ChoreographerInput::Stop);
        let choreographer = choreographer.join().map_err(Error::RunChoreographer);
        let builder = builder.join().map_err(Error::RunBuilder);
        builder.and(choreographer)
    }
}

impl Drop for ScenePipeline {
    fn drop(&mut self) {
        let _failure = self.stop();
    }
}

impl SceneSink<'_> {
    /// Whether any frame of the view ended into the sink for the surface. A restart does not reset this, because the
    /// slot for the surface contains the built scene until the next frame ends.
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
        self.state.surface.apply(change.clone());
        self.push(SceneItem::Surface(change))?;
        if self.state.open == OpenFrame::WithoutState {
            self.state.open = OpenFrame::WithState;
        }
        Ok(())
    }

    /// Ends the open frame when it contains a change of the surface state or the sink ends frames with a sequence,
    /// unless the frame waits for the whole scene. Returns whether the frame ended.
    ///
    /// # Errors
    ///
    /// Fails when the choreographer is gone.
    pub fn end_state_frame(&mut self) -> Result<bool, Closed> {
        let ends = match self.state.open {
            OpenFrame::WithState => true,
            OpenFrame::WithoutState => self.end.is_some(),
            OpenFrame::AwaitingScene => false,
        };
        if ends {
            self.end_open_frame()?;
        }
        Ok(ends)
    }

    fn push(&self, item: SceneItem) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Change(self.surface, Box::new(item)))
    }

    fn end_open_frame(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
        self.state.open = OpenFrame::WithoutState;
        Ok(())
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
        self.end_open_frame()?;
        self.state.any_frame_ended = true;
        Ok(())
    }

    /// Drops the frames that wait for the scene builder, and sends the whole surface state again, because the dropped
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
