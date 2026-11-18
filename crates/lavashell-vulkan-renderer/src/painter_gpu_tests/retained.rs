// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_renderer::Fonts;
use lavashell_renderer::test_damage::{
    SURFACE, card, damage_frames, full_damage_then_cuts, repainted_pixels_on,
};

#[test]
fn a_stable_leaf_without_cache_room_neither_fills_nor_widens_the_damage() -> Result<(), TestError> {
    let first = card(Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let second = card(Rect::new(100.dp(), 0.dp(), 40.dp(), 40.dp()))?;
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    let frames = damage_frames(full_damage_then_cuts(), || {
        Ok(vec![first.clone(), second.clone()])
    })?;
    let mut target = gpu.offscreen(SURFACE.pixel_size())?;
    let one_copy = 40 * 40 * target.image_info().bytes_per_pixel();
    target
        .canvas()
        .direct_context()
        .ok_or(TestError::Missing("GPU context of the canvas"))?
        .set_resource_cache_limit(one_copy);
    let shown = repainted_pixels_on(&mut target, &mut painter, None, frames)?;
    let (cut, first_and_cut) = (80 * 10, 110 * 41);
    assert_eq!(shown, [SURFACE.pixel_count(), first_and_cut, cut, cut]);
    let stats = painter.render_stats();
    assert_eq!((stats.cache_misses, stats.cache_hits), (1, 2));
    assert_eq!(stats.uncached_leaves, 2 + 3);
    Ok(())
}
