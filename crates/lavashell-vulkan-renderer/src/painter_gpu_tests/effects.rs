// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_renderer::effect_checks::{
    assert_continuous_glass, assert_neutral_glass, draw, sample, sample_in_middle_row,
};
use lavashell_renderer::test_surfaces::{ChannelRounding, EFFECT_SURFACE};

#[test]
fn vulkan_f16_effects_preserve_hdr_headroom_and_sharp_foreground() -> Result<(), TestError> {
    let mut painter = VulkanRenderer::headless()?;
    let mut s = painter.offscreen(EFFECT_SURFACE.pixel_size())?;
    draw(&mut s, |d| {
        d.fill(
            EFFECT_SURFACE.bounds(),
            Color::linear([2.0, 0.125, 0.5], 1.0, Gamut::Srgb),
        );
        d.backdrop(
            Shape::rounded(Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()), 8.dp()),
            constant!(Backdrop::blur(Dp::new(3.0))),
        );
        d.fill(
            Rect::new(40.dp(), 20.dp(), 4.dp(), 4.dp()),
            Color::rgb(0x00ff_0000),
        );
    })?;
    let hdr = sample_in_middle_row(&mut s, 24);
    for (channel, expected) in hdr.into_iter().zip([2.0, 0.125]) {
        let rounding = ChannelRounding::of(&s)?.after(3, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{hdr:?}");
    }
    let fg = sample(&mut s, 41, 21);
    assert!(fg[0] > 0.98 && fg[1] < 0.01);
    assert_neutral_glass(&mut s)?;
    Ok(assert_continuous_glass(&mut s)?)
}
