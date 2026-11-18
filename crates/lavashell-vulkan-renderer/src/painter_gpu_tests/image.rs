// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_renderer::Fonts;
use lavashell_renderer::test_images::{IMAGE, assert_color, encoded, scene};
use skia_safe::EncodedImageFormat;

#[test]
fn jpeg_and_png_pixels_survive_vulkan_upload() -> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        let commands = scene(encoded(format)?)?;
        let mut surface = gpu.offscreen(IMAGE.pixel_size())?;
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut surface)?;
        assert_color(&png, format)?;
    }
    Ok(())
}
