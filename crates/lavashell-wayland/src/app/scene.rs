// SPDX-License-Identifier: GPL-3.0-only
use super::{Duration, Viewport, VulkanRenderer};
use crate::Error;
use lavashell_renderer::vulkan::{
    CompletedScene, SceneCompletion, SceneImage, SceneMailbox, SceneSource,
};
use std::sync::Arc;

pub(super) struct SceneRuntime {
    source: Box<dyn SceneSource>,
    /// Limits the scenes in flight to the smallest number of frames in flight among the swapchains that show the
    /// scene. The mailbox is created once the first of those swapchains exists.
    mailbox: Option<SceneMailbox<SceneImage, SceneCompletion>>,
}

impl SceneRuntime {
    pub fn new(source: Box<dyn SceneSource>) -> Self {
        Self {
            source,
            mailbox: None,
        }
    }

    /// The time at which the source produces its next scene, or `None` while the mailbox is full or not created.
    /// Completed scenes need no wake, because `tick` reads them at every frame and frames follow while scenes
    /// arrive.
    pub fn wake(&self) -> Option<Duration> {
        (self.mailbox.as_ref())
            .filter(|mailbox| mailbox.can_publish())
            .map(|_| self.source.next_deadline())
    }

    /// Reads the completed scenes and asks the source for the next one when it is due. The mailbox allows as many
    /// scenes in flight as `frames_in_flight`, the smallest number of frames in flight among the swapchains that
    /// show the scene.
    pub fn tick(
        &mut self,
        gpu: &mut VulkanRenderer,
        now: Duration,
        viewport: Viewport,
        frames_in_flight: Option<usize>,
    ) -> Result<(), Error> {
        (gpu.poll_scene_imports()).map_err(Error::ReleaseSceneImports)?;
        if let Some(capacity) = frames_in_flight {
            match &mut self.mailbox {
                Some(mailbox) => mailbox.set_capacity(capacity),
                None => self.mailbox = Some(SceneMailbox::new(capacity)),
            }
        }
        let Some(mailbox) = &mut self.mailbox else {
            return Ok(());
        };
        (mailbox.poll(SceneCompletion::status)).map_err(Error::SceneCompletion)?;
        if now >= self.source.next_deadline()
            && mailbox.can_publish()
            && let Some(pending) =
                (self.source.produce(gpu, now, viewport)).map_err(Error::ProduceScene)?
        {
            (mailbox.publish(pending.scene, pending.completion)).map_err(Error::PublishScene)?;
            // A scene made from an external frame that has already completed needs only an acquire on the
            // local queue. Polling again makes it the latest scene in this frame instead of the next tick.
            (mailbox.poll(SceneCompletion::status)).map_err(Error::SceneCompletion)?;
        }
        Ok(())
    }

    pub fn latest(&self) -> Option<Arc<CompletedScene<SceneImage>>> {
        self.mailbox.as_ref().and_then(SceneMailbox::latest)
    }
}
