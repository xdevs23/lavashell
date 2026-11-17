// SPDX-License-Identifier: GPL-3.0-only

use super::extract::ExtractedOverlay;
use crate::{Error, ShowFrameError};
use bevy::prelude::*;
use lavashell_actor::{Closed, Latest, SystemClock};
use lavashell_core::frame::FrameTime;
use lavashell_core::geometry::Viewport;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::surface::{SurfaceId, SurfaceIds};
use lavashell_core::units::Density;
use lavashell_core::view::{FrameError, TextMeasurer, View};
use lavashell_error_chain::message_chain;
use lavashell_runtime::scene_pipeline::{BuilderReport, ScenePipeline};
use lavashell_runtime::scheduler::FrameSequence;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Resource)]
pub struct OverlayFrame {
    surface: SurfaceId,
    scene: ScenePipeline,
    restart: Arc<Latest<ApplyError>>,
    sent: BTreeMap<FrameSequence, SentFrame>,
    pub sequence: u64,
}

#[derive(Clone, Copy)]
pub(super) struct SentFrame {
    pub(super) sequence: u64,
    pub(super) density: Density,
    pub(super) presentation: Duration,
}

impl OverlayFrame {
    fn start() -> Result<Self, Error> {
        let restart = Arc::new(Latest::new());
        let published = Arc::clone(&restart);
        let observer = move |report: BuilderReport| -> Result<(), Closed> {
            if let BuilderReport::Restart { error, .. } = report {
                published.publish(error);
            }
            Ok(())
        };
        Ok(Self {
            surface: SurfaceIds::default()
                .next_id()
                .map_err(Error::NumberOverlaySurface)?,
            scene: ScenePipeline::start(Box::new(observer), &SystemClock::start())
                .map_err(Error::StartScene)?,
            restart,
            sent: BTreeMap::new(),
            sequence: 0,
        })
    }

    #[must_use]
    pub fn needs_frame(&self, view: &View) -> bool {
        view.needs_frame() || self.restart.read().is_some()
    }

    pub fn show_frame_at(
        &mut self,
        view: &mut View,
        viewport: Viewport,
        text: &mut impl TextMeasurer,
        timing: FrameTime,
    ) -> Result<(), ShowFrameError> {
        if let Some(error) = self.restart.take() {
            self.scene.restart_view(self.surface, view, &error);
        }
        let end = FrameSequence(self.sent.last_key_value().map_or(0, |(last, _)| last.0 + 1));
        let mut sink = self.scene.sink(self.surface, Some(end));
        let failures = match view.frame_into_at(viewport, text, timing, &mut sink) {
            Ok(failures) => failures,
            Err(FrameError::Mirror(error)) => return Err(ShowFrameError::Mirror(error)),
            Err(FrameError::Packet(error)) => return Err(ShowFrameError::Packet(error)),
            Err(FrameError::NumberSceneNode(error)) => {
                return Err(ShowFrameError::NumberSceneNode(error));
            }
            Err(FrameError::Sink(Closed)) => {
                return Err(match self.scene.stop() {
                    Err(failure) => ShowFrameError::Scene(failure),
                    Ok(()) => ShowFrameError::SceneStopped,
                });
            }
        };
        for failure in failures {
            log::error!("{}", message_chain(failure));
        }
        if let Some(built) = self.scene.built(self.surface).sequence {
            self.sent = self.sent.split_off(&built);
        }
        let sent = SentFrame {
            sequence: self.sequence,
            density: viewport.density,
            presentation: timing.presentation,
        };
        let _earlier_frame_with_the_same_end_is_superseded = self.sent.insert(end, sent);
        Ok(())
    }

    pub(super) fn extracted(&self) -> ExtractedOverlay {
        let built = self.scene.built(self.surface);
        let sent = (built.sequence).and_then(|sequence| self.sent.get(&sequence).copied());
        ExtractedOverlay {
            surface: self.surface,
            sent,
            built,
        }
    }
}

pub(super) struct StartOverlayScene;

impl Command<Result<(), Error>> for StartOverlayScene {
    fn apply(self, world: &mut World) -> Result<(), Error> {
        world.insert_resource(OverlayFrame::start()?);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
