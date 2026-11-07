// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameSequence;
use crate::builder_actor::BuilderInput;
use crate::scene_item::{SceneFrame, SceneItem};
use lavashell_actor::{Actor, Choreographer, Closed, Effect, Flow, Intake, Merge, Sender};
use lavashell_core::scene::tree::ChangeKey;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// What the views and the scene builder send to the choreographer.
pub(crate) enum ChoreographerInput {
    /// A change in the open frame of the surface.
    Change(SurfaceId, Box<SceneItem>),
    /// Ends the open frame of the surface, with the sequence of the composed frame or without one.
    End(SurfaceId, Option<FrameSequence>),
    Restart(SurfaceId),
    Retire(SurfaceId),
    /// The scene builder requests the next frames, with the node count of each scene that it built.
    Ask(Vec<(SurfaceId, usize)>),
    Stop,
}

impl Merge for ChoreographerInput {
    type Key = (SurfaceId, ChangeKey);

    fn effect(&self) -> Effect<(SurfaceId, ChangeKey)> {
        match self {
            Self::Change(surface, item) => item.effect().map(|key| (*surface, key)),
            Self::End(..) | Self::Restart(_) | Self::Retire(_) | Self::Ask(_) | Self::Stop => {
                Effect::BuildsOn
            }
        }
    }

    /// The scene builder and the pipeline reach the choreographer also while the views wait.
    fn passes_closed_intake(&self) -> bool {
        match self {
            Self::Change(_, item) => item.passes_closed_intake(),
            Self::End(..) | Self::Restart(_) | Self::Retire(_) => false,
            Self::Ask(_) | Self::Stop => true,
        }
    }
}

/// Merges the frames for each surface until the scene builder requests them. The views wait while the frames waiting
/// for a surface hold more items that build on earlier items than the built scene for that surface has nodes. Past
/// that count, sending the whole scene again would cost less.
pub(crate) struct ChoreographerActor {
    surfaces: BTreeMap<SurfaceId, SurfaceFrames>,
    asked: bool,
    builder: Sender<BuilderInput>,
    intake: Intake<ChoreographerInput>,
}

#[derive(Default)]
struct SurfaceFrames {
    frames: Choreographer<SceneItem, FrameSequence>,
    /// The node count of the scene that the builder last built, or `None` after a restart until the builder built the
    /// restarted scene.
    nodes: Option<usize>,
    /// Whether the surface restarted and the builder has not taken the frame that follows the restart.
    restarted: bool,
}

impl ChoreographerActor {
    pub(crate) fn new(builder: Sender<BuilderInput>, intake: Intake<ChoreographerInput>) -> Self {
        Self {
            surfaces: BTreeMap::new(),
            asked: true,
            builder,
            intake,
        }
    }

    fn surface_frames(&mut self, surface: SurfaceId) -> &mut SurfaceFrames {
        self.surfaces.entry(surface).or_default()
    }

    /// Sends the waiting frames to the scene builder once it requested them. Stops when the builder is gone.
    fn send_frames_to_builder(&mut self) -> Flow {
        if !self.asked {
            return Flow::Continue;
        }
        let frames: Vec<_> = (self.surfaces.iter_mut())
            .filter_map(|(&surface, waiting)| Some((surface, waiting.take()?)))
            .collect();
        if frames.is_empty() {
            return Flow::Continue;
        }
        self.asked = false;
        match self.builder.push(BuilderInput::Frames(frames)) {
            Ok(()) => Flow::Continue,
            Err(Closed) => Flow::Stop,
        }
    }

    /// Closes the intake for the views while the frames waiting for any surface are full, and opens it otherwise.
    fn open_or_close_view_intake(&self) {
        if (self.surfaces.values()).any(SurfaceFrames::full) {
            self.intake.close();
        } else {
            self.intake.open();
        }
    }
}

impl Actor for ChoreographerActor {
    type Input = ChoreographerInput;
    type Error = Infallible;

    fn handle(&mut self, input: ChoreographerInput) -> Result<Flow, Infallible> {
        match input {
            ChoreographerInput::Change(surface, item) => {
                self.surface_frames(surface).frames.push(*item);
                return Ok(Flow::Continue);
            }
            ChoreographerInput::End(surface, sequence) => {
                self.surface_frames(surface).frames.end_frame(sequence);
            }
            ChoreographerInput::Restart(surface) => self.surface_frames(surface).restart(),
            ChoreographerInput::Retire(surface) => {
                let _retired = self.surfaces.remove(&surface);
                if self.builder.push(BuilderInput::Retire(surface)).is_err() {
                    return Ok(Flow::Stop);
                }
            }
            ChoreographerInput::Ask(sizes) => {
                for (surface, nodes) in sizes {
                    if let Some(built) = self.surfaces.get_mut(&surface) {
                        built.built(nodes);
                    }
                }
                self.asked = true;
            }
            ChoreographerInput::Stop => return Ok(Flow::Stop),
        }
        let flow = self.send_frames_to_builder();
        self.open_or_close_view_intake();
        Ok(flow)
    }
}

impl SurfaceFrames {
    fn restart(&mut self) {
        self.frames.restart();
        self.nodes = None;
        self.restarted = true;
    }

    fn take(&mut self) -> Option<SceneFrame> {
        let frame = self.frames.take()?;
        if frame.restart {
            self.restarted = false;
        }
        Some(frame)
    }

    /// Records the node count of the scene built from the frames taken last. While the builder has not taken the frame
    /// that follows a restart, the count belongs to the scene before the restart and is ignored.
    fn built(&mut self, nodes: usize) {
        if !self.restarted {
            self.nodes = Some(nodes);
        }
    }

    /// Whether the waiting frames hold more items that build on earlier items than the built scene has nodes.
    fn full(&self) -> bool {
        (self.nodes).is_some_and(|nodes| self.frames.waiting_builds_on() > nodes)
    }
}

#[cfg(test)]
mod tests;
