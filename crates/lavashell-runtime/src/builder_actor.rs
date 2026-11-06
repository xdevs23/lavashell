// SPDX-License-Identifier: GPL-3.0-only
use crate::choreographer_actor::ChoreographerInput;
use crate::scene_item::{SceneFrame, SceneItem};
use crate::scene_pipeline::{BuilderObserver, BuilderReport};
use crate::surface_slots::{BuiltScene, SurfaceSlots};
use lavashell_actor::{Actor, Effect, Flow, Merge, Sender};
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::scene::{ChangeSink, SceneBuilder};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::sync::Arc;

/// What the choreographer sends to the scene builder.
pub(crate) enum BuilderInput {
    /// The frames for each surface that the choreographer merged since the builder requested them.
    Frames(Vec<(SurfaceId, SceneFrame)>),
    Retire(SurfaceId),
}

impl Merge for BuilderInput {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}

/// Builds the scene for each surface from the frames received from the choreographer, stores each scene in the slot
/// for that surface, and requests the next frames.
pub(crate) struct BuilderActor {
    scenes: BTreeMap<SurfaceId, Scene>,
    slots: Arc<SurfaceSlots>,
    choreographer: Sender<ChoreographerInput>,
    observer: BuilderObserver,
}

/// The scene for a surface. A stale scene failed to build a frame, and skips each frame until a frame restarts it.
#[derive(Default)]
struct Scene {
    builder: SceneBuilder,
    stale: bool,
}

impl BuilderActor {
    pub(crate) fn new(
        slots: Arc<SurfaceSlots>,
        choreographer: Sender<ChoreographerInput>,
        observer: BuilderObserver,
    ) -> Self {
        Self {
            scenes: BTreeMap::new(),
            slots,
            choreographer,
            observer,
        }
    }
}

impl Actor for BuilderActor {
    type Input = BuilderInput;
    type Error = ApplyError;

    /// Fails when a frame does not fit a scene that has shown nothing yet.
    fn handle(&mut self, input: BuilderInput) -> Result<Flow, ApplyError> {
        let frames = match input {
            BuilderInput::Frames(frames) => frames,
            BuilderInput::Retire(surface) => {
                let _retired = self.scenes.remove(&surface);
                self.slots.retire(surface);
                return Ok(Flow::Continue);
            }
        };
        let mut reports = Vec::new();
        let mut sizes = Vec::new();
        for (surface, frame) in frames {
            let sequence = frame.end;
            let scene = self.scenes.entry(surface).or_default();
            match scene.apply(frame) {
                Ok(Some(size)) => {
                    let built = BuiltScene {
                        sequence,
                        snapshot: scene.builder.snapshot(),
                    };
                    self.slots.publish(surface, built);
                    sizes.push((surface, size));
                }
                Ok(None) => {}
                Err(error) if scene.builder.is_empty() => return Err(error),
                Err(error) => reports.push(BuilderReport::Restart { surface, error }),
            }
            reports.push(BuilderReport::Built { surface, sequence });
        }
        let observed = reports.into_iter().try_for_each(&mut self.observer);
        if observed.is_err() {
            return Ok(Flow::Stop);
        }
        // The mailbox can still contain surfaces that the choreographer retired before it stopped.
        let _choreographer_gone = self.choreographer.push(ChoreographerInput::Ask(sizes));
        Ok(Flow::Continue)
    }
}

impl Drop for BuilderActor {
    /// Stops the choreographer, so that a view that waits for the intake of the choreographer fails once the builder
    /// is gone.
    fn drop(&mut self) {
        let _choreographer_gone = self.choreographer.push(ChoreographerInput::Stop);
    }
}

impl Scene {
    /// Builds `frame` into the scene and returns the node count of the built scene. Skips the frame and returns `None`
    /// while the scene is stale and the frame does not restart it.
    fn apply(&mut self, frame: SceneFrame) -> Result<Option<usize>, ApplyError> {
        if self.stale && !frame.restart {
            return Ok(None);
        }
        let built = build(&mut self.builder, frame);
        self.stale = built.is_err();
        built.map(|()| Some(self.builder.size()))
    }
}

fn build(builder: &mut SceneBuilder, frame: SceneFrame) -> Result<(), ApplyError> {
    if frame.restart {
        builder.restart()?;
    }
    for SceneItem(change) in frame.items {
        builder.change(change)?;
    }
    builder.end_frame()
}

#[cfg(test)]
mod tests;
