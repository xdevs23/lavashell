// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_renderer::paint::parent_copy_checks::{
    assert_backdrop_reads_only_isolating_layers, assert_backdrop_reads_through,
    assert_layer_bounds_cut_the_backdrop,
};
use lavashell_renderer::test_surfaces::EFFECT_SURFACE;

#[test]
fn vulkan_backdrop_reads_through_a_source_over_effect_layer() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut surface = gpu.offscreen(EFFECT_SURFACE.pixel_size())?;
    assert_backdrop_reads_through(&mut surface)?;
    assert_backdrop_reads_only_isolating_layers(&mut surface)?;
    Ok(assert_layer_bounds_cut_the_backdrop(&mut surface)?)
}
