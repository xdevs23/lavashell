// SPDX-License-Identifier: GPL-3.0-only
use super::HeadlessBackend;
use crate::Error;
use crate::vulkan::{PendingScene, SceneSource, VulkanRenderer};
use lavashell_core::prelude::ScaledViewport;
use lavashell_platform::RendererBackend;
use std::time::Duration;

struct EmptyScene;

impl SceneSource for EmptyScene {
    fn next_deadline(&self) -> Duration {
        Duration::ZERO
    }

    fn produce(
        &mut self,
        _: &mut VulkanRenderer,
        _: Duration,
        _: ScaledViewport,
    ) -> Result<Option<PendingScene>, Error> {
        Ok(None)
    }
}

#[test]
fn a_backend_shows_a_scene_only_once_one_is_set() {
    let backend = HeadlessBackend::without_surfaces();
    assert!(!backend.shows_scene());
    let with_scene = backend.with_scene(Box::new(|| Box::new(EmptyScene)));
    assert!(with_scene.shows_scene());
}
