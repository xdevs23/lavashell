// SPDX-License-Identifier: GPL-3.0-only
use crate::builder_actor::BuilderInput;
use crate::scene_item::{SceneFrame, SceneItem, SceneKey};
use crate::scheduler::FrameSequence;
use lavashell_actor::{Actor, Closed, Flow, FrameBatcher, Intake, Merge, MergeEffect, Sender};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::convert::Infallible;

pub(crate) enum ChoreographerInput {
    Change(SurfaceId, Box<SceneItem>),
    End(SurfaceId, Option<FrameSequence>),
    Restart(SurfaceId),
    Retire(SurfaceId),
    Ask(Vec<(SurfaceId, usize)>),
    Stop,
}

impl Merge for ChoreographerInput {
    type Key = (SurfaceId, SceneKey);

    fn effect(&self) -> MergeEffect<(SurfaceId, SceneKey)> {
        match self {
            Self::Change(surface, item) => item.effect().map(|key| (*surface, key)),
            Self::End(..) | Self::Restart(_) | Self::Retire(_) | Self::Ask(_) | Self::Stop => {
                MergeEffect::Appends
            }
        }
    }

    fn passes_closed_intake(&self) -> bool {
        match self {
            Self::Change(_, item) => item.passes_closed_intake(),
            Self::End(..) | Self::Restart(_) | Self::Retire(_) => false,
            Self::Ask(_) | Self::Stop => true,
        }
    }
}

pub(crate) struct ChoreographerActor {
    surfaces: BTreeMap<SurfaceId, SurfaceFrames>,
    asked: bool,
    builder: Sender<BuilderInput>,
    intake: Intake<ChoreographerInput>,
}

#[derive(Default)]
struct SurfaceFrames {
    frames: FrameBatcher<SceneItem, FrameSequence>,
    nodes: Option<usize>,
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

    fn built(&mut self, nodes: usize) {
        if !self.restarted {
            self.nodes = Some(nodes);
        }
    }

    fn full(&self) -> bool {
        (self.nodes).is_some_and(|nodes| self.frames.waiting_appended_items() > nodes)
    }
}

#[cfg(test)]
mod tests;
