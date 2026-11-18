// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_renderer::paint::opacity_checks::{assert_first_frame_blur, assert_nested_groups};
use lavashell_renderer::test_surfaces::EFFECT_SURFACE;

#[test]
fn vulkan_backdrop_blurs_on_the_first_partial_opacity_frame() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    assert_first_frame_blur(&mut *gpu.offscreen(EFFECT_SURFACE.pixel_size())?)?;
    assert_nested_groups(|| Ok(gpu.offscreen(EFFECT_SURFACE.pixel_size())?))
}
