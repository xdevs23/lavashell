// SPDX-License-Identifier: GPL-3.0-only
//! Scene images rendered by a host engine and drawn as the base layer under the UI. Submitting a scene depends neither
//! on the effect quality of the UI nor on Wayland frame callbacks.

use super::{CompletedScene, Completion, Status, VulkanRenderer};
use crate::Error;
use lavashell_core::prelude::Viewport;
use skia_safe::gpu::{DirectContext, DirectContextId, SyncCpu};
use skia_safe::{Canvas, Surface};
use std::time::Duration;

/// The completion of the GPU work that writes a scene image. It is `None` for an imported image, whose writes were
/// complete before the import.
pub struct SceneCompletion(Option<Completion>);

impl SceneCompletion {
    /// The completion of an imported image, whose writes by the producer are already complete. The acquire barrier
    /// and every later Skia read are submitted in order on the queue of the importer, so nothing is left to wait for.
    pub(super) fn already_written() -> Self {
        Self(None)
    }

    /// # Errors
    /// Fails when the device is lost.
    pub fn status(&self) -> Result<Status, Error> {
        self.0
            .as_ref()
            .map_or(Ok(Status::Complete), Completion::status)
    }
}

/// A scene image sampled by the renderer. For an imported image, it also has the lease on the imported allocation. A
/// `SceneImage` never hands the image out, so no copy of the image outlives the lease.
pub struct SceneImage {
    importer: DirectContextId,
    /// Declared before the lease, so the image drops first.
    image: skia_safe::Image,
    _external: Option<super::external::Lease>,
}

impl SceneImage {
    pub(super) fn new(
        importer: &DirectContext,
        image: skia_safe::Image,
        external: Option<super::external::Lease>,
    ) -> Self {
        Self {
            importer: importer.id(),
            image,
            _external: external,
        }
    }

    /// The image as the base layer of `canvas`.
    ///
    /// # Errors
    /// Fails when `canvas` belongs to a Skia context other than the context of the renderer that imported the image.
    /// Only that context can draw the image.
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

/// A scene image and a canvas of the Skia context that imported it.
pub(crate) struct SceneLayer<'a> {
    image: &'a skia_safe::Image,
    canvas: &'a Canvas,
}

impl SceneLayer<'_> {
    /// Fills the base layer of the canvas with the image.
    pub(crate) fn draw(&self) {
        let layer = skia_safe::Rect::from_isize(self.canvas.base_layer_size());
        let paint = skia_safe::Paint::default();
        let _chained_canvas = self.canvas.draw_image_rect(self.image, None, layer, &paint);
    }
}

pub struct PendingScene {
    pub scene: CompletedScene<SceneImage>,
    pub completion: SceneCompletion,
}

/// A host engine that produces scene images and schedules its own scene work. When `produce` returns `None`, the last
/// completed image is shown again and the UI clock does not change.
pub trait SceneSource {
    fn next_deadline(&self) -> Duration;

    /// Submits the scene work that is ready and returns without waiting. The renderer calls this function in sequence
    /// with its own submissions to the queue, so the function must not wait for a CPU simulation or for asset
    /// preparation. `viewport` is the surface size in dp, with the density that converts the size to pixels.
    ///
    /// # Errors
    /// Returns the failure of the renderer to produce the scene.
    fn produce(
        &mut self,
        gpu: &mut VulkanRenderer,
        now: Duration,
        viewport: Viewport,
    ) -> Result<Option<PendingScene>, Error>;
}

impl VulkanRenderer {
    /// Creates a snapshot image of `surface` and submits a completion signal after all scene writes on this queue.
    /// Later draws into `surface` do not change the snapshot, because Skia copies the resources of a snapshot before
    /// it writes to them.
    ///
    /// # Errors
    /// Returns the failure to create the completion of the scene writes.
    pub fn submit_scene(
        &mut self,
        surface: &mut Surface,
        generation: u64,
        produced_at: Duration,
    ) -> Result<PendingScene, Error> {
        let image = surface.image_snapshot();
        self.context
            .flush_and_submit_surface(surface, Some(SyncCpu::No));
        let completion = Completion::after_submitted(&self.device)?;
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
