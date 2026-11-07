// SPDX-License-Identifier: GPL-3.0-only
use crate::choreographer_actor::ChoreographerInput;
use crate::scene_item::{SceneFrame, SceneItem};
use crate::scene_pipeline::{BuilderObserver, BuilderReport, BuiltChange, BuiltScene};
use crate::surface_slots::SurfaceSlots;
use lavashell_actor::{Actor, Effect, Flow, Merge, Sender};
use lavashell_core::scene::tree::{ApplyError, Change};
use lavashell_core::scene::{ChangeSink, SceneBuilder};
use lavashell_core::surface::{SurfaceId, SurfaceState};
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

/// Builds the scene and the surface state for each surface from the frames received from the choreographer, stores
/// them in the slot for that surface, and requests the next frames.
pub(crate) struct BuilderActor {
    scenes: BTreeMap<SurfaceId, Scene>,
    slots: Arc<SurfaceSlots>,
    choreographer: Sender<ChoreographerInput>,
    observer: BuilderObserver,
}

/// The scene and the surface state for a surface. A stale scene failed to build a frame, and skips the scene changes
/// of each frame until a frame restarts it.
#[derive(Default)]
struct Scene {
    builder: SceneBuilder,
    state: SurfaceState,
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
            let (change, built) = scene.apply(frame);
            if change != BuiltChange::Nothing {
                self.slots.publish(surface, scene.built());
            }
            match built {
                Ok(Some(size)) => sizes.push((surface, size)),
                Ok(None) => {}
                Err(error) if scene.builder.is_empty() => return Err(error),
                Err(error) => reports.push(BuilderReport::Restart { surface, error }),
            }
            reports.push(BuilderReport::Built {
                surface,
                sequence,
                change,
            });
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
    /// Applies every change of the surface state in `frame`, and builds the changes of the scene in `frame`. Returns
    /// what the frame changed, and the node count of the scene once the frame ended in the builder.
    fn apply(&mut self, frame: SceneFrame) -> (BuiltChange, Result<Option<usize>, ApplyError>) {
        let mut changes = Vec::new();
        let mut state_changed = false;
        for item in frame.items {
            match item {
                SceneItem::Change(change) => changes.push(change),
                SceneItem::Surface(change) => {
                    self.state.apply(change);
                    state_changed = true;
                }
            }
        }
        let built = self.build(frame.restart, changes);
        let change = match built {
            Ok(Some(_)) => BuiltChange::Scene,
            Ok(None) | Err(_) if state_changed => BuiltChange::State,
            Ok(None) | Err(_) => BuiltChange::Nothing,
        };
        (change, built)
    }

    /// Builds `changes` into the scene, after a restart of the scene when `restart` is set, and returns the node count
    /// of the built scene. The frame ends in the builder only when it restarts or changes the scene. Returns `None`
    /// when the frame does not end in the builder, or when the scene is stale and the frame does not restart it.
    fn build(&mut self, restart: bool, changes: Vec<Change>) -> Result<Option<usize>, ApplyError> {
        if !restart && (self.stale || changes.is_empty()) {
            return Ok(None);
        }
        let built = build(&mut self.builder, restart, changes);
        self.stale = built.is_err();
        built.map(|()| Some(self.builder.size()))
    }

    fn built(&self) -> BuiltScene {
        BuiltScene {
            snapshot: self.builder.snapshot(),
            state: self.state.clone(),
        }
    }
}

fn build(
    builder: &mut SceneBuilder,
    restart: bool,
    changes: Vec<Change>,
) -> Result<(), ApplyError> {
    if restart {
        builder.restart()?;
    }
    for change in changes {
        builder.change(change)?;
    }
    builder.end_frame()
}

#[cfg(test)]
mod tests;
