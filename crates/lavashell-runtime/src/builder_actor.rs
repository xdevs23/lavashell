// SPDX-License-Identifier: GPL-3.0-only
use crate::choreographer_actor::ChoreographerInput;
use crate::scene_item::{SceneFrame, SceneItem};
use crate::scene_pipeline::{BuilderObserver, BuilderReport};
use crate::scheduler::FrameSequence;
use crate::surface_slots::{BuiltSurface, SurfaceSlots};
use lavashell_actor::{Actor, Effect, Flow, Merge, Sender};
use lavashell_core::scene::tree::ApplyError;
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
    sequence: Option<FrameSequence>,
    state: SurfaceState,
    stale: bool,
}

/// What a frame changed for a surface.
struct Applied {
    /// The node count of the scene once the frame built it, or `None` when the frame left the scene as it was.
    scene: Result<Option<usize>, ApplyError>,
    state_changed: bool,
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
            let Applied {
                scene: built,
                state_changed,
            } = scene.apply(frame);
            let report = match (sequence, &built) {
                (None, Ok(None)) if state_changed => BuilderReport::StateChanged { surface },
                _ => BuilderReport::Built { surface, sequence },
            };
            let publish = state_changed || matches!(built, Ok(Some(_)));
            match built {
                Ok(Some(size)) => sizes.push((surface, size)),
                Ok(None) => {}
                Err(error) if scene.builder.is_empty() => return Err(error),
                Err(error) => reports.push(BuilderReport::Restart { surface, error }),
            }
            if publish {
                self.slots.publish(surface, scene.built());
            }
            reports.push(report);
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
    /// Applies every change of the surface state in `frame`, and builds its scene changes into the scene until one of
    /// them fails. Skips the scene changes while the scene is stale and the frame does not restart it. A frame without
    /// scene changes leaves the scene as it was.
    fn apply(&mut self, frame: SceneFrame) -> Applied {
        let builds = frame.restart || !self.stale;
        let mut scene_changed = frame.restart;
        let mut state_changed = false;
        let mut built = if frame.restart {
            self.builder.restart()
        } else {
            Ok(())
        };
        for item in frame.items {
            match item {
                SceneItem::Surface(change) => {
                    state_changed = true;
                    self.state.apply(change);
                }
                SceneItem::Change(change) => {
                    scene_changed = true;
                    if builds && built.is_ok() {
                        built = self.builder.change(change);
                    }
                }
            }
        }
        if !builds || !scene_changed {
            return Applied {
                scene: Ok(None),
                state_changed,
            };
        }
        let built = built.and_then(|()| self.builder.end_frame());
        self.stale = built.is_err();
        if built.is_ok() {
            self.sequence = frame.end;
        }
        Applied {
            scene: built.map(|()| Some(self.builder.size())),
            state_changed,
        }
    }

    fn built(&self) -> BuiltSurface {
        BuiltSurface {
            sequence: self.sequence,
            scene: self.builder.snapshot(),
            state: self.state.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
