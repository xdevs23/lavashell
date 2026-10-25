// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_surfaces::pixels;
use crate::vulkan::OffscreenSurface;
use crate::{Fonts, Painter, StreamFrames, VulkanRenderer};
use lavashell_core::prelude::*;
use skia_safe::{Color as SkColor, IPoint, Surface};

const SIDE: i32 = 32;

fn filled(gpu: &mut VulkanRenderer, color: SkColor) -> Result<OffscreenSurface, TestError> {
    let side = u32::try_from(SIDE)?;
    let mut surface = gpu.offscreen((side, side))?;
    let _chained_canvas = surface.canvas().clear(color);
    Ok(surface)
}

fn paint(
    gpu: &mut VulkanRenderer,
    painter: &mut Painter,
    streams: &StreamFrames,
    image: StreamImage,
) -> Result<OffscreenSurface, TestError> {
    let mut target = filled(gpu, SkColor::TRANSPARENT)?;
    let whole = Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp());
    let commands = [DrawCommand::StreamImage {
        image,
        destination: whole,
        frame: whole,
    }];
    painter.paint_streams_as_leaf(target.canvas(), &commands, Density::ONE, streams)?;
    Ok(target)
}

/// The sRGB bytes of the pixel in the middle of `target`, read after a submit that waits for the GPU.
fn middle(target: &mut Surface) -> Result<[u8; 4], TestError> {
    let bytes = pixels(target);
    let index = usize::try_from((SIDE / 2 * SIDE + SIDE / 2) * 4)?;
    Ok(bytes[index..index + 4].try_into()?)
}

#[test]
fn a_rebind_or_retire_after_a_paint_leaves_the_reads_queued_by_that_paint_intact()
-> Result<(), TestError> {
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    let mut streams = StreamFrames::default();
    let stream = StreamId::next();
    let side = u32::try_from(SIDE)?;
    let image = |generation| StreamImage {
        stream,
        generation,
        size: PixelSize::new(side, side),
    };
    let mut red = filled(&mut gpu, SkColor::RED)?;
    streams.bind(&image(1), red.image_snapshot().into(), IPoint::new(0, 0));
    let mut first = paint(&mut gpu, &mut painter, &streams, image(1))?;
    let mut blue = filled(&mut gpu, SkColor::BLUE)?;
    streams.bind(&image(2), blue.image_snapshot().into(), IPoint::new(0, 0));
    drop(red);
    assert_eq!(middle(&mut first)?, [255, 0, 0, 255]);
    let mut second = paint(&mut gpu, &mut painter, &streams, image(2))?;
    streams.retire(stream);
    drop(blue);
    assert_eq!(middle(&mut second)?, [0, 0, 255, 255]);
    let mut retired = paint(&mut gpu, &mut painter, &streams, image(2))?;
    assert_eq!(middle(&mut retired)?, [0, 0, 0, 0]);
    assert_eq!(painter.failures.count(DrawFailure::StreamUnbound), 1);
    painter.retire_gpu_resources();
    Ok(())
}
