// SPDX-License-Identifier: GPL-3.0-only
use super::super::{PixelGeometry, TextDestination};
use super::*;
use crate::test_error::TestError;
use skia_safe::{AlphaType, ColorSpace, ColorType, Data, Image, ImageInfo};

fn read(png: &[u8]) -> Result<Vec<u8>, TestError> {
    let image = Image::from_encoded(Data::new_copy(png)).ok_or(Error::DecodeImage)?;
    let info = ImageInfo::new(
        image.dimensions(),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(image.read_pixels(
        &info,
        &mut bytes,
        info.min_row_bytes(),
        (0, 0),
        skia_safe::image::CachingHint::Disallow
    ));
    Ok(bytes)
}

fn edges(bytes: &[u8], width: usize, left: usize, top: usize, height: usize) -> usize {
    (top..top + height)
        .flat_map(|y| (left..left + 280).map(move |x| (y * width + x) * 4))
        .filter(|i| {
            bytes[*i].abs_diff(bytes[*i + 1]) > 2 || bytes[*i + 1].abs_diff(bytes[*i + 2]) > 2
        })
        .count()
}

#[test]
fn translucent_rgb_coverage_survives_actual_vulkan_paint_with_valid_alpha_and_clock_fallback()
-> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&fonts)?.with_raster_paths(true);
    painter.set_blend_space(crate::BlendSpace::Srgb);
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
    assert!(painter.resources.tiles.borrow().images().count() <= 1024);
    painter.retire_gpu_resources();
    Ok(())
}

type GeometryOutput = (&'static str, usize, usize, Vec<u8>);

/// Draws the sample commands with Vulkan once for each pixel geometry and compares the pixels with a CPU drawing.
/// Returns for each geometry its name, the number of pixels with colored edges in the upper and in the lower part
/// of the transparent column, and the pixels.
fn geometry_outputs(
    gpu: &mut crate::VulkanRenderer,
    painter: &mut crate::Painter,
) -> Result<Vec<GeometryOutput>, TestError> {
    let commands = super::samples::commands();
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
        let mut native = gpu.offscreen((900, 340))?;
        let _chained_canvas = native.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(native.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut native)?;
        let bytes = read(&png)?;
        assert!(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[..3].iter().all(|v| *v <= p[3]))
        );
        let info = native.image_info();
        let mut cpu =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let _chained_canvas = cpu.canvas().clear(skia_safe::Color::TRANSPARENT);
        let mut reference = super::tests::new_painter(geometry)?;
        reference.set_blend_space(crate::BlendSpace::Srgb);
        reference.paint_as_leaf(cpu.canvas(), &commands, Density::ONE)?;
        let expected = super::tests::pixels(
            &mut cpu,
            &ImageInfo::new(
                (900, 340),
                ColorType::RGBA8888,
                AlphaType::Premul,
                ColorSpace::new_srgb(),
            ),
        );
        let error = bytes
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .ok_or(TestError::Missing("pixels"))?;
        assert!(
            error <= 4,
            "actual Vulkan/CPU {name} coverage differs by {error} bytes"
        );
        outputs.push((
            name,
            edges(&bytes, 900, 0, 0, 170),
            edges(&bytes, 900, 0, 176, 150),
            bytes,
        ));
    }
    Ok(outputs)
}

fn assert_unsafe_scopes_fall_back(
    gpu: &mut crate::VulkanRenderer,
    painter: &mut crate::Painter,
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
        let mut style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
        style.color = Color::WHITE.with_alpha(0.4);
        commands.push(DrawCommand::Text {
            text: "Headphones p".into(),
            rect: Rect::new(7.25.dp(), 4.5.dp(), 280.dp(), 36.dp()),
            style,
        });
        commands.push(DrawCommand::Restore);
        let mut draw = |geometry| -> Result<Vec<u8>, TestError> {
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
            let mut target = gpu.offscreen((300, 48))?;
            let _chained_canvas = target.canvas().clear(skia_safe::Color::TRANSPARENT);
            painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
            read(&gpu.png(&mut target)?)
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
