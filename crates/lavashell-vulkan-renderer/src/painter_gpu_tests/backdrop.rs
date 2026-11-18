// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_renderer::effect_registry::backdrop_cache_checks::assert_cached_glass_matches_ordinary_glass;
use lavashell_renderer::effect_registry::backdrop_hole_checks::{
    a_frosted_and_dimmed_card_shows_the_lens_sharp_on,
    every_blurring_variant_leaves_the_lens_sharp_on,
    the_cached_variant_replays_its_capture_for_a_moving_lens_on,
};
use lavashell_renderer::test_surfaces::EFFECT_SURFACE;

#[test]
fn vulkan_cached_glass_preserves_optics_and_fractional_geometry() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    for clip in [false, true] {
        assert_cached_glass_matches_ordinary_glass(
            &mut *gpu.offscreen(EFFECT_SURFACE.pixel_size())?,
            &mut *gpu.offscreen(EFFECT_SURFACE.pixel_size())?,
            clip,
        )?;
    }
    Ok(())
}

#[test]
fn vulkan_backdrops_leave_the_lens_sharp() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut surface = || {
        gpu.offscreen(EFFECT_SURFACE.pixel_size())
            .map_err(TestError::VulkanRenderer)
    };
    every_blurring_variant_leaves_the_lens_sharp_on(&mut surface)?;
    the_cached_variant_replays_its_capture_for_a_moving_lens_on(&mut surface)?;
    a_frosted_and_dimmed_card_shows_the_lens_sharp_on(&mut surface)
}
