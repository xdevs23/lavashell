// SPDX-License-Identifier: GPL-3.0-only
use crate::builder_actor::BuilderActor;
use crate::choreographer_actor::{ChoreographerActor, ChoreographerInput};
use crate::scene_item::SceneItem;
use crate::surface_slots::SurfaceSlots;
use crate::{Error, FrameSequence};
use lavashell_actor::{Closed, Handle, Sender, mailbox, spawn};
use lavashell_core::scene::tree::{ApplyError, Change, SceneSnapshot};
use lavashell_core::scene::{ChangeSink, SinkId};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::sync::Arc;

/// What the scene builder reports about the scene for a surface.
#[derive(Clone, Copy, Debug)]
pub enum BuilderReport {
    /// The scene contains every frame that the view composed up to the frame with `sequence`. A scene built from a
    /// frame sent outside a compose has no sequence.
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
    },
    /// A frame did not fit the scene. The scene shows the last frame that it built, until the view sends the whole
    /// scene to a restarted sink.
    Restart {
        surface: SurfaceId,
        error: ApplyError,
    },
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

/// The change sink that sends one frame from the view of a surface to the choreographer.
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

    /// The sink for the next frame from the view of `surface`, which ends the frame with `end`.
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

    /// The newest scene that the scene builder built for `surface`.
    #[must_use]
    pub fn snapshot(&self, surface: SurfaceId) -> Arc<SceneSnapshot> {
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
    /// Whether any frame ended into the sink for the surface. A restart keeps the built scene until the next frame
    /// ends, so it does not reset this.
    #[must_use]
    pub fn any_frame_ended(&self) -> bool {
        self.state.any_frame_ended
    }
}

impl ChangeSink for SceneSink<'_> {
    type Error = Closed;

    fn id(&self) -> SinkId {
        self.state.id
    }

    fn change(&mut self, change: Change) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Change(
            self.surface,
            Box::new(SceneItem(change)),
        ))
    }

    fn end_frame(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::End(self.surface, self.end))?;
        self.state.any_frame_ended = true;
        Ok(())
    }

    fn restart(&mut self) -> Result<(), Closed> {
        (self.choreographer).push(ChoreographerInput::Restart(self.surface))
    }
}

#[cfg(test)]
mod tests;
