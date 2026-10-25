// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_scenes::leaves;
use crate::test_surfaces::{pattern, pixels, surface};
use crate::{Fonts, Painter, StreamFrames};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Packet;
use skia_safe::{IPoint, IRect};
use std::sync::Arc;
use std::time::Duration;

const SIDE: i32 = 96;

fn scene(paint: impl FnOnce(&mut Drawing<'_>)) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    let bounds = Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp());
    let context = DrawContext::new(Viewport::new(bounds.size(), Density::ONE));
    paint(&mut Drawing::new(&mut commands, bounds, &context));
    commands
}

fn render(
    painter: &mut Painter,
    commands: &[DrawCommand],
    streams: &StreamFrames,
) -> Result<Vec<u8>, TestError> {
    let mut target = surface(SIDE, SIDE)?;
    painter.paint_streams_as_leaf(target.canvas(), commands, Density::ONE, streams)?;
    Ok(pixels(&mut target))
}

fn transparent(bytes: &[u8]) -> bool {
    bytes.chunks(4).all(|pixel| pixel[3] == 0)
}

fn frame_of(
    stream: StreamId,
    generation: u64,
    pixels: &skia_safe::Image,
) -> Result<StreamImage, TestError> {
    Ok(StreamImage {
        stream,
        generation,
        size: PixelSize::new(
            u32::try_from(pixels.width())?,
            u32::try_from(pixels.height())?,
        ),
    })
}

type Pane = fn(&mut Drawing<'_>, &dyn Fn(&mut Drawing<'_>));

#[test]
fn a_bound_frame_draws_the_pixels_of_the_same_image_in_every_pane() -> Result<(), TestError> {
    let pixels = pattern(64, 32)?;
    let image = Image::encoded(Vec::new(), 64, 32).with_backend_data(pixels.clone());
    let frame = frame_of(StreamId::next(), 1, &pixels)?;
    let mut streams = StreamFrames::default();
    streams.bind(&frame, pixels.into(), IPoint::new(0, 0));
    let destination = Rect::new(8.dp(), 8.dp(), 48.dp(), 48.dp());
    let open: Pane = |drawing, content| content(drawing);
    let rounded: Pane = |drawing, content| {
        drawing.save();
        drawing.clip(Rect::new(8.dp(), 8.dp(), 48.dp(), 48.dp()), 16.dp());
        content(drawing);
        drawing.restore();
    };
    let translucent: Pane = |drawing, content| {
        drawing.commands.push(DrawCommand::Opacity(0.5));
        content(drawing);
        drawing.restore();
    };
    // A display with pixels that are not square scales the whole pane by different factors.
    let stretched: Pane = |drawing, content| drawing.transformed(Affine::scale(1.5, 0.75), content);
    let cases = [
        ("contain", open, ImageFit::contain()),
        ("cover", open, ImageFit::cover()),
        ("16 dp corners", rounded, ImageFit::cover()),
        ("opacity", translucent, ImageFit::cover()),
        ("non-uniform scale", stretched, ImageFit::contain()),
    ];
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for (case, pane, fit) in cases {
        let stream = |drawing: &mut Drawing<'_>| drawing.stream_image(&frame, destination, fit);
        let still = |drawing: &mut Drawing<'_>| drawing.image(&image, destination, fit);
        let streamed = scene(|drawing| pane(drawing, &stream));
        let imaged = scene(|drawing| pane(drawing, &still));
        let drawn = render(&mut painter, &streamed, &streams)?;
        assert!(!transparent(&drawn), "{case}");
        assert!(drawn == render(&mut painter, &imaged, &streams)?, "{case}");
    }
    assert_eq!(painter.failures.count(DrawFailure::StreamUnbound), 0);
    assert_eq!(painter.failures.count(DrawFailure::StreamStale), 0);
    Ok(())
}

#[test]
fn a_crop_draws_the_pixels_of_that_part_of_the_buffer_alone() -> Result<(), TestError> {
    let buffer = pattern(64, 48)?;
    let crop = IRect::from_xywh(8, 4, 32, 24);
    let part = buffer
        .make_subset(None, crop, skia_safe::image::RequiredProperties::default())
        .ok_or(TestError::Missing("cropped image"))?;
    let image = Image::encoded(Vec::new(), 32, 24).with_backend_data(part);
    let frame = StreamImage {
        stream: StreamId::next(),
        generation: 1,
        size: PixelSize::new(32, 24),
    };
    let mut streams = StreamFrames::default();
    streams.bind(&frame, buffer.into(), IPoint::new(crop.left, crop.top));
    let destination = Rect::new(8.dp(), 8.dp(), 64.dp(), 64.dp());
    let zoomed = ImageFrame::new(-50.pct(), -50.pct(), 200.pct(), 200.pct());
    let small = ImageFrame::new(8.dp(), 4.dp(), 16.dp(), 12.dp());
    // The box extends past the crop on two sides, on no side and on four sides.
    let fits = [
        ImageFit::contain(),
        ImageFit::Frame(zoomed),
        ImageFit::Frame(small),
    ];
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for fit in fits {
        let streamed = scene(|drawing| drawing.stream_image(&frame, destination, fit));
        let imaged = scene(|drawing| drawing.image(&image, destination, fit));
        let drawn = render(&mut painter, &streamed, &streams)?;
        assert!(!transparent(&drawn), "{fit:?}");
        assert!(drawn == render(&mut painter, &imaged, &streams)?, "{fit:?}");
    }
    Ok(())
}

#[test]
fn a_frame_under_a_non_uniform_scale_fills_the_scaled_destination() -> Result<(), TestError> {
    let pixels = pattern(32, 32)?;
    let frame = frame_of(StreamId::next(), 1, &pixels)?;
    let mut streams = StreamFrames::default();
    streams.bind(&frame, pixels.into(), IPoint::new(0, 0));
    let commands = scene(|drawing| {
        drawing.transformed(Affine::scale(1.5, 0.75), |drawing| {
            let destination = Rect::new(8.dp(), 8.dp(), 32.dp(), 32.dp());
            drawing.stream_image(&frame, destination, ImageFit::cover());
        });
    });
    let drawn = render(
        &mut Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?,
        &commands,
        &streams,
    )?;
    // The destination covers x from 12 to 60 and y from 6 to 30 on the canvas.
    let alpha = |x: i32, y: i32| -> Result<u8, TestError> {
        Ok(drawn[usize::try_from((y * SIDE + x) * 4 + 3)?])
    };
    for (x, y) in [(12, 6), (59, 6), (12, 29), (59, 29)] {
        assert_eq!(alpha(x, y)?, 255, "({x}, {y})");
    }
    for (x, y) in [(11, 6), (12, 5), (60, 29), (59, 30)] {
        assert_eq!(alpha(x, y)?, 0, "({x}, {y})");
    }
    Ok(())
}

#[test]
fn a_stream_without_a_bound_frame_draws_nothing_and_records_a_failure() -> Result<(), TestError> {
    let frame = frame_of(StreamId::next(), 1, &pattern(32, 32)?)?;
    let commands = scene(|drawing| {
        let destination = Rect::new(0.dp(), 0.dp(), 32.dp(), 32.dp());
        drawing.stream_image(&frame, destination, ImageFit::cover());
    });
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    assert!(transparent(&render(
        &mut painter,
        &commands,
        &StreamFrames::default()
    )?));
    assert_eq!(painter.failures.count(DrawFailure::StreamUnbound), 1);
    let mut target = surface(SIDE, SIDE)?;
    painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
    assert!(transparent(&pixels(&mut target)));
    assert_eq!(painter.failures.count(DrawFailure::StreamUnbound), 2);
    Ok(())
}

#[test]
fn a_frame_of_another_generation_than_the_bound_one_draws_nothing_and_records_a_failure()
-> Result<(), TestError> {
    let pixels = pattern(32, 32)?;
    let stream = StreamId::next();
    let mut streams = StreamFrames::default();
    streams.bind(
        &frame_of(stream, 1, &pixels)?,
        pixels.clone().into(),
        IPoint::new(0, 0),
    );
    let newer = frame_of(stream, 2, &pixels)?;
    let commands = scene(|drawing| {
        let destination = Rect::new(0.dp(), 0.dp(), 32.dp(), 32.dp());
        drawing.stream_image(&newer, destination, ImageFit::cover());
    });
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    assert!(transparent(&render(&mut painter, &commands, &streams)?));
    assert_eq!(painter.failures.count(DrawFailure::StreamStale), 1);
    streams.retire(stream);
    assert!(transparent(&render(&mut painter, &commands, &streams)?));
    assert_eq!(painter.failures.count(DrawFailure::StreamUnbound), 1);
    Ok(())
}

#[test]
fn a_frame_in_a_leaf_is_drawn_on_every_paint_and_never_cached() -> Result<(), TestError> {
    let pixels = pattern(48, 48)?;
    let frame = frame_of(StreamId::next(), 1, &pixels)?;
    let mut streams = StreamFrames::default();
    streams.bind(&frame, pixels.into(), IPoint::new(0, 0));
    let destination = Rect::new(8.dp(), 8.dp(), 48.dp(), 48.dp());
    let direct = scene(|drawing| drawing.stream_image(&frame, destination, ImageFit::cover()));
    let tree = leaves([Packet::new(Arc::from(direct.clone()))])?;
    let expected = render(
        &mut Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?,
        &direct,
        &streams,
    )?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    for _ in 0..3 {
        let mut target = surface(SIDE, SIDE)?;
        painter.paint_scene_with_streams(
            target.canvas(),
            tree.at(Duration::ZERO),
            Density::ONE,
            &streams,
        );
        assert_eq!(crate::test_surfaces::pixels(&mut target), expected);
    }
    let stats = painter.render_stats();
    assert_eq!(
        (stats.cache_hits, stats.cache_misses, stats.uncached_leaves),
        (0, 0, 3)
    );
    Ok(())
}
