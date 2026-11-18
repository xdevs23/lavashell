// SPDX-License-Identifier: GPL-3.0-only
use crate::VulkanRenderer;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_renderer::test_font::probe_style;
use lavashell_renderer::test_surfaces::{Pixels, encoded_pixels, pixels};
use lavashell_renderer::text::approximate_samples::{
    SAMPLE_SURFACE, commands, large_text_rows, new_painter, small_text_rows,
    transparent_backdrop_columns,
};
use lavashell_renderer::text::sample_fixtures::LABEL_SURFACE;
use lavashell_renderer::text::{PixelGeometry, TextAntialiasing, TextDestination, TextRendering};
use lavashell_renderer::{BlendSpace, Painter, RasterMode};
use skia_safe::AlphaType;
use std::ops::Range;

const LARGEST_CHANNEL_SPREAD_OF_A_GRAY_PIXEL: u8 = 2;

fn count_colored_edge_pixels(
    pixels: &Pixels<[u8; 4]>,
    columns: &Range<u16>,
    rows: &Range<u16>,
) -> usize {
    rows.clone()
        .flat_map(|y| columns.clone().map(move |x| pixels.at(x, y)))
        .filter(|[red, green, blue, _]| {
            red.abs_diff(*green) > LARGEST_CHANNEL_SPREAD_OF_A_GRAY_PIXEL
                || green.abs_diff(*blue) > LARGEST_CHANNEL_SPREAD_OF_A_GRAY_PIXEL
        })
        .count()
}

#[test]
fn translucent_rgb_coverage_survives_actual_vulkan_paint_with_valid_alpha_and_clock_fallback()
-> Result<(), TestError> {
    let fonts = lavashell_renderer::test_font::test_font()?;
    let mut gpu = VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&fonts)?.with_raster_mode(RasterMode::Tiles);
    painter.set_blend_space(BlendSpace::Srgb);
    let outputs = geometry_outputs(&mut gpu, &mut painter)?;
    assert_eq!(outputs[0].1, 0, "default text acquired RGB edges");
    assert!(
        outputs[1].1 > 20 && outputs[2].1 > 20,
        "missing small-text RGB/BGR coverage"
    );
    assert_ne!(
        outputs[1].3, outputs[2].3,
        "RGB/BGR masks were reused across geometry changes"
    );
    assert!(
        outputs.iter().all(|result| result.2 == 0),
        "100px raster clock promised unavailable LCD"
    );
    assert_unsafe_scopes_fall_back(&mut gpu, &mut painter)?;
    assert!(painter.retained_tile_count() <= 1024);
    painter.retire_gpu_resources();
    Ok(())
}

type GeometryOutput = (&'static str, usize, usize, Pixels<[u8; 4]>);

fn geometry_outputs(
    gpu: &mut VulkanRenderer,
    painter: &mut Painter,
) -> Result<Vec<GeometryOutput>, TestError> {
    let commands = commands();
    let columns = transparent_backdrop_columns()?;
    let (small_rows, large_rows) = (small_text_rows()?, large_text_rows()?);
    let mut outputs = Vec::new();
    let cases = [
        ("gray", PixelGeometry::None),
        ("rgb", PixelGeometry::Rgb),
        ("bgr", PixelGeometry::Bgr),
    ];
    for (name, geometry) in cases {
        painter.set_text_rendering(
            TextRendering {
                antialiasing: TextAntialiasing::SubpixelApproximate,
                ..Default::default()
            },
            TextDestination {
                opaque: false,
                geometry,
            },
        );
        let mut native = gpu.offscreen(SAMPLE_SURFACE.pixel_size())?;
        let _canvas_returned_for_chaining = native.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(native.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut native)?;
        let actual = encoded_pixels(&png, AlphaType::Premul)?;
        assert!(actual.iter().all(|p| p[..3].iter().all(|v| *v <= p[3])));
        let info = native.image_info();
        let mut cpu =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let _canvas_returned_for_chaining = cpu.canvas().clear(skia_safe::Color::TRANSPARENT);
        let mut reference = new_painter(geometry)?;
        reference.set_blend_space(BlendSpace::Srgb);
        reference.paint_as_leaf(cpu.canvas(), &commands, Density::ONE)?;
        let expected = pixels(&mut cpu);
        let error = actual.largest_channel_difference(&expected)?;
        assert!(
            error <= 4,
            "actual Vulkan/CPU {name} coverage differs by {error} bytes"
        );
        outputs.push((
            name,
            count_colored_edge_pixels(&actual, &columns, &small_rows),
            count_colored_edge_pixels(&actual, &columns, &large_rows),
            actual,
        ));
    }
    Ok(outputs)
}

fn assert_unsafe_scopes_fall_back(
    gpu: &mut VulkanRenderer,
    painter: &mut Painter,
) -> Result<(), TestError> {
    for (case, prefix) in [
        (
            "fractional",
            vec![
                DrawCommand::Save,
                DrawCommand::Transform(Affine::translate(Point::new(0.25.dp(), 0.5.dp()))),
            ],
        ),
        ("isolated", vec![DrawCommand::Opacity(0.5)]),
    ] {
        let mut commands = prefix;
        let mut style = probe_style();
        style.color = Color::WHITE.with_alpha(0.4);
        commands.push(DrawCommand::Text {
            text: "Headphones p".into(),
            rect: Rect::new(7.25.dp(), 4.5.dp(), 280.dp(), 36.dp()),
            style,
        });
        commands.push(DrawCommand::Restore);
        let mut draw = |geometry| -> Result<Pixels<[u8; 4]>, TestError> {
            painter.set_text_rendering(
                TextRendering {
                    antialiasing: TextAntialiasing::SubpixelApproximate,
                    ..Default::default()
                },
                TextDestination {
                    opaque: false,
                    geometry,
                },
            );
            let mut target = gpu.offscreen(LABEL_SURFACE.pixel_size())?;
            let _canvas_returned_for_chaining =
                target.canvas().clear(skia_safe::Color::TRANSPARENT);
            painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
            Ok(encoded_pixels(&gpu.png(&mut target)?, AlphaType::Premul)?)
        };
        let reference = draw(PixelGeometry::None)?;
        assert_eq!(
            draw(PixelGeometry::Rgb)?,
            reference,
            "unsafe {case} escaped fallback"
        );
    }
    Ok(())
}
