// SPDX-License-Identifier: GPL-3.0-only

use super::{CompletedScene, Completion, OffscreenSurface, Status, VulkanRenderer};
use crate::Error;
use lavashell_core::prelude::ScaledViewport;
use skia_safe::Canvas;
use skia_safe::gpu::{DirectContext, DirectContextId, SyncCpu};
use std::time::Duration;

pub struct SceneCompletion(Option<Completion>);

impl SceneCompletion {
    pub(super) fn already_written() -> Self {
        Self(None)
    }

    pub fn status(&self) -> Result<Status, Error> {
        self.0
            .as_ref()
            .map_or(Ok(Status::Complete), Completion::status)
    }
}

pub struct SceneImage {
    importer: DirectContextId,
    image: skia_safe::Image,
    lease: Option<super::external::Lease>,
}

impl SceneImage {
    pub(super) fn new(
        importer: &DirectContext,
        image: skia_safe::Image,
        lease: Option<super::external::Lease>,
    ) -> Self {
        Self {
            importer: importer.id(),
            image,
            lease,
        }
    }

    pub(crate) fn layer_of<'a>(&'a self, canvas: &'a Canvas) -> Result<SceneLayer<'a>, Error> {
        let canvas_owner = canvas.direct_context().map(|context| context.id());
        if canvas_owner != Some(self.importer) {
            return Err(Error::SceneOfAnotherRenderer {
                importer: self.importer,
                canvas_owner,
            });
        }
        Ok(SceneLayer {
            image: &self.image,
            canvas,
        })
    }
}

impl Drop for SceneImage {
    fn drop(&mut self) {
        drop(self.lease.take());
    }
}

pub(crate) struct SceneLayer<'a> {
    image: &'a skia_safe::Image,
    canvas: &'a Canvas,
}

impl SceneLayer<'_> {
    pub(crate) fn draw(&self) {
        let layer = skia_safe::Rect::from_isize(self.canvas.base_layer_size());
        let paint = skia_safe::Paint::default();
        let saved = self.canvas.save();
        let _canvas_returned_for_chaining = self
            .canvas
            .reset_matrix()
            .draw_image_rect(self.image, None, layer, &paint)
            .restore_to_count(saved);
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
