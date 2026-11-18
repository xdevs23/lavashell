// SPDX-License-Identifier: GPL-3.0-only

use super::{CompletedScene, Completion, OffscreenSurface, VulkanRenderer};
use crate::Error;
use lavashell_core::prelude::ScaledViewport;
use lavashell_renderer::{CompletionStatus, SceneImage};
use skia_safe::gpu::SyncCpu;
use std::time::Duration;

pub struct SceneCompletion(Option<Completion>);

impl SceneCompletion {
    pub(super) fn already_written() -> Self {
        Self(None)
    }

    pub fn status(&self) -> Result<CompletionStatus, Error> {
        self.0
            .as_ref()
            .map_or(Ok(CompletionStatus::Complete), Completion::status)
    }
}

pub struct PendingScene {
    pub scene: CompletedScene<SceneImage>,
    pub completion: SceneCompletion,
}

pub trait SceneSource {
    fn next_deadline(&self) -> Duration;

    fn produce(
        &mut self,
        gpu: &mut VulkanRenderer,
        now: Duration,
        scaled_viewport: ScaledViewport,
    ) -> Result<Option<PendingScene>, Error>;
}

impl VulkanRenderer {
    pub fn submit_scene(
        &mut self,
        surface: &mut OffscreenSurface,
        generation: u64,
        produced_at: Duration,
    ) -> Result<PendingScene, Error> {
        if !surface.belongs_to(self.context.device()) {
            return Err(Error::ForeignTarget);
        }
        let image = surface.image_snapshot();
        self.context
            .flush_and_submit_surface(surface, Some(SyncCpu::No));
        let completion = Completion::after_submitted(self.context.device())?;
        Ok(PendingScene {
            scene: CompletedScene {
                generation,
                produced_at,
                content: SceneImage::new(&self.context, image, None),
            },
            completion: SceneCompletion(Some(completion)),
        })
    }
}

#[cfg(test)]
mod tests;
