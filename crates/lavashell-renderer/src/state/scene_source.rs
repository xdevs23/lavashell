// SPDX-License-Identifier: GPL-3.0-only
use super::StateError;
use crate::vulkan::{
    CompletedScene, SceneCompletion, SceneImage, SceneMailbox, SceneSource, VulkanRenderer,
};
use lavashell_core::prelude::Viewport;
use std::sync::Arc;
use std::time::Duration;

pub(super) struct SceneImages {
    source: Box<dyn SceneSource>,
    mailbox: Option<SceneMailbox<SceneImage, SceneCompletion>>,
}

impl SceneImages {
    pub fn new(source: Box<dyn SceneSource>) -> Self {
        Self {
            source,
            mailbox: None,
        }
    }

    pub fn wake(&self) -> Option<Duration> {
        (self.mailbox.as_ref())
            .filter(|mailbox| mailbox.can_publish())
            .map(|_| self.source.next_deadline())
    }

    pub fn tick(
        &mut self,
        gpu: &mut VulkanRenderer,
        now: Duration,
        viewport: Viewport,
        frames_in_flight: Option<usize>,
    ) -> Result<(), StateError> {
        (gpu.poll_scene_imports()).map_err(StateError::ReleaseSceneImports)?;
        if let Some(capacity) = frames_in_flight {
            match &mut self.mailbox {
                Some(mailbox) => mailbox.set_capacity(capacity),
                None => self.mailbox = Some(SceneMailbox::new(capacity)),
            }
        }
        let Some(mailbox) = &mut self.mailbox else {
            return Ok(());
        };
        (mailbox.poll(SceneCompletion::status)).map_err(StateError::SceneCompletion)?;
        if now >= self.source.next_deadline()
            && mailbox.can_publish()
            && let Some(pending) =
                (self.source.produce(gpu, now, viewport)).map_err(StateError::ProduceScene)?
        {
            (mailbox.publish(pending.scene, pending.completion))
                .map_err(StateError::PublishScene)?;
            (mailbox.poll(SceneCompletion::status)).map_err(StateError::SceneCompletion)?;
        }
        Ok(())
    }

    pub fn latest(&self) -> Option<Arc<CompletedScene<SceneImage>>> {
        self.mailbox.as_ref().and_then(SceneMailbox::latest)
    }
}
