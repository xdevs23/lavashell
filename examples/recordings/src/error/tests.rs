// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell::renderer::Error as RendererCause;
use lavashell::vulkan_renderer::Error as VulkanRendererError;

#[test]
fn a_vulkan_renderer_that_cannot_start_asks_for_a_vulkan_driver() {
    let cause = RendererCause::DuplicateFontAlias("body".to_owned());
    let started = StartError::State(StateError::CreateRenderer(
        VulkanRendererError::CreatePainter(cause),
    ));
    let failed = PlatformRunError::StartRenderer(SpawnError::Start(RendererError::Start(started)));
    assert!(matches!(
        Error::Record(failed).next_step(),
        NextStep::ProvideVulkanDriver
    ));
}
