// SPDX-License-Identifier: GPL-3.0-only
use crate::BlendSpace;
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_surfaces::floats;
use crate::text::{PixelGeometry, TextAntialiasing, TextDestination, TextRendering};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};

fn srgb_painter() -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::static_test_faces()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    painter.set_blend_space(BlendSpace::Srgb);
    Ok(painter)
}

const SURFACE_WIDTH: usize = 300;

const FONT_SIZE: f64 = 13.0;

const ONE_LINE: Rect = Rect::new(Dp::new(7.0), Dp::new(2.0), Dp::new(280.0), Dp::new(40.0));

fn text_in_weight(rect: Rect, weight: f64, color: Color) -> Result<DrawCommand, TestError> {
    Ok(DrawCommand::Text {
        text: "Mon 5 Oct office".into(),
        rect,
        style: TextStyle::new("probe", Sp::new(FONT_SIZE), color)
            .weight(FontWeight::try_new(weight)?)
            .resolve_unscaled(),
    })
}

fn extended_srgb_channels(
    painter: &mut crate::Painter,
    commands: &[DrawCommand],
) -> Result<Vec<f32>, TestError> {
    let info = ImageInfo::new(
        (i32::try_from(SURFACE_WIDTH)?, 48),
        ColorType::RGBAF32,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    painter.paint_as_leaf(surface.canvas(), commands, Density::ONE)?;
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)));
    Ok((bytes.as_chunks::<4>().0.iter())
        .map(|channel| floats::<1>(channel)[0])
        .collect())
}

fn lighter_and_heavier_face(rect: Rect, color: Color) -> Result<Vec<f32>, TestError> {
    let mut reference = srgb_painter()?;
    let lighter_face = extended_srgb_channels(
        &mut reference,
        &[text_in_weight(
            rect,
            400.0,
            color.with_alpha(color.alpha() * 0.75),
        )?],
    )?;
    let heavier_face = extended_srgb_channels(
        &mut reference,
        &[text_in_weight(
            rect,
            700.0,
            color.with_alpha(color.alpha() * 0.25),
        )?],
    )?;
    Ok((lighter_face.iter().zip(&heavier_face))
        .map(|(lighter, heavier)| lighter + heavier)
        .collect())
}

#[test]
fn a_weight_blend_keeps_a_display_p3_color_beyond_srgb_while_blending_in_srgb()
-> Result<(), TestError> {
    let green = Color::encoded([0.0, 1.0, 0.0], 1.0, Gamut::DisplayP3);
    let both_faces = lighter_and_heavier_face(ONE_LINE, green)?;
    assert!(both_faces.iter().any(|channel| *channel > 1.0));
    let between = text_in_weight(ONE_LINE, 475.0, green)?;
    assert_eq!(
        extended_srgb_channels(&mut srgb_painter()?, &[between])?,
        both_faces
    );
    Ok(())
}

#[test]
fn a_weight_blend_paints_the_lines_of_a_paragraph_that_overflow_its_box() -> Result<(), TestError> {
    let short_box = Rect::new(7.dp(), 2.dp(), 60.dp(), 4.dp());
    let both_faces = lighter_and_heavier_face(short_box, Color::WHITE)?;
    let lowest_inked_row = (both_faces.chunks(SURFACE_WIDTH * 4))
        .rposition(|row| row.iter().any(|channel| *channel > 0.0))
        .ok_or(TestError::Missing("ink of the paragraph"))?;
    let box_widened_by_the_font_size = short_box.outset(Dp::new(FONT_SIZE));
    let widened_bottom = box_widened_by_the_font_size.y + box_widened_by_the_font_size.height;
    assert!(f64::from(u32::try_from(lowest_inked_row)?) > widened_bottom.get());
    let between = text_in_weight(short_box, 475.0, Color::WHITE)?;
    assert_eq!(
        extended_srgb_channels(&mut srgb_painter()?, &[between])?,
        both_faces
    );
    Ok(())
}

#[test]
fn a_weight_blend_scaled_to_nothing_draws_nothing_and_records_no_failure() -> Result<(), TestError>
{
    let mut painter = srgb_painter()?;
    let scaled_to_nothing = [
        DrawCommand::Transform(Affine::scale(0.0, 0.0)),
        text_in_weight(ONE_LINE, 475.0, Color::WHITE)?,
    ];
    let channels = extended_srgb_channels(&mut painter, &scaled_to_nothing)?;
    assert!(channels.iter().all(|channel| *channel == 0.0));
    assert_eq!(painter.failures.count(DrawFailure::SingularTransform), 0);
    Ok(())
}

fn on_lcd_canvas_half_a_pixel_off(
    painter: &mut crate::Painter,
    backdrop: skia_safe::Color,
    commands: &[DrawCommand],
) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        (i32::try_from(SURFACE_WIDTH)?, 48),
        ColorType::BGRA8888,
        AlphaType::Opaque,
        ColorSpace::new_srgb(),
    );
    let props = lcd_destination().surface_props();
    let mut surface =
        skia_safe::surfaces::raster(&info, None, Some(&props)).ok_or(TestError::RasterSurface)?;
    let canvas = surface.canvas();
    let _canvas_returned_for_chaining = canvas.clear(backdrop).translate((0.5, 0.0));
    painter.paint_as_leaf(canvas, commands, Density::ONE)?;
    let mut pixels = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0)));
    Ok(pixels)
}

fn lcd_destination() -> TextDestination {
    TextDestination {
        opaque: true,
        geometry: PixelGeometry::Rgb,
    }
}

fn lcd_painter() -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::static_test_faces()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    let subpixel = TextRendering {
        antialiasing: TextAntialiasing::Subpixel,
        ..Default::default()
    };
    painter.set_text_rendering(subpixel, lcd_destination());
    Ok(painter)
}

fn has_colored_fringe(image: &[u8]) -> bool {
    (image.as_chunks::<4>().0.iter()).any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
}

fn faces_in_shares(backdrop: &[u8], [lighter, heavier]: [&[u8]; 2], shares: [f64; 2]) -> Vec<f64> {
    let [lighter_share, heavier_share] = shares;
    (backdrop.iter().zip(lighter).zip(heavier))
        .map(|((backdrop, lighter), heavier)| {
            let [backdrop, lighter, heavier] = [backdrop, lighter, heavier].map(|&v| f64::from(v));
            backdrop + lighter_share * (lighter - backdrop) + heavier_share * (heavier - backdrop)
        })
        .collect()
}

const ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS: f64 = 2.0;

fn alike(actual: &[u8], expected: &[f64]) -> bool {
    (actual.iter().zip(expected)).all(|(actual, expected)| {
        (f64::from(*actual) - expected).abs() <= ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS
    })
}

const LIGHT_BACKDROP: skia_safe::Color = skia_safe::Color::from_rgb(236, 232, 224);

#[test]
fn lcd_text_that_skia_draws_at_rest_stays_lcd_through_a_weight_blend() -> Result<(), TestError> {
    let mut painter = lcd_painter()?;
    for weight in [400.0, 475.0, 700.0] {
        let text = text_in_weight(ONE_LINE, weight, Color::WHITE)?;
        let image = on_lcd_canvas_half_a_pixel_off(&mut painter, skia_safe::Color::BLACK, &[text])?;
        assert!(has_colored_fringe(&image));
    }
    Ok(())
}

#[test]
fn lcd_text_in_any_color_blends_the_faces_at_rest_in_their_shares() -> Result<(), TestError> {
    let dark = Color::encoded([0.12, 0.16, 0.24], 1.0, Gamut::Srgb);
    let green = Color::encoded([0.0, 1.0, 0.0], 1.0, Gamut::DisplayP3);
    let mut painter = lcd_painter()?;
    for (color, backdrop) in [
        (Color::WHITE, skia_safe::Color::BLACK),
        (dark, LIGHT_BACKDROP),
        (green, LIGHT_BACKDROP),
    ] {
        let mut drawn = |commands: &[DrawCommand]| {
            on_lcd_canvas_half_a_pixel_off(&mut painter, backdrop, commands)
        };
        let empty = drawn(&[])?;
        let lighter = drawn(&[text_in_weight(ONE_LINE, 400.0, color)?])?;
        let heavier = drawn(&[text_in_weight(ONE_LINE, 700.0, color)?])?;
        let between = drawn(&[text_in_weight(ONE_LINE, 475.0, color)?])?;
        let one_weight_off = drawn(&[text_in_weight(ONE_LINE, 401.0, color)?])?;
        assert!(alike(
            &between,
            &faces_in_shares(&empty, [&lighter, &heavier], [0.75, 0.25])
        ));
        let lighter_alone = faces_in_shares(&empty, [&lighter, &heavier], [1.0, 0.0]);
        assert!(alike(&one_weight_off, &lighter_alone));
    }
    Ok(())
}

#[test]
fn a_weight_blend_in_an_opacity_group_on_an_lcd_canvas_adds_both_faces_in_grayscale()
-> Result<(), TestError> {
    let mut painter = lcd_painter()?;
    let black = on_lcd_canvas_half_a_pixel_off(&mut painter, skia_safe::Color::BLACK, &[])?;
    let mut in_half_opacity = |weight, alpha| -> Result<Vec<u8>, TestError> {
        let commands = [
            DrawCommand::Opacity(0.5),
            text_in_weight(ONE_LINE, weight, Color::WHITE.with_alpha(alpha))?,
            DrawCommand::Restore,
        ];
        on_lcd_canvas_half_a_pixel_off(&mut painter, skia_safe::Color::BLACK, &commands)
    };
    let between = in_half_opacity(475.0, 1.0)?;
    let faded_faces = [in_half_opacity(400.0, 0.75)?, in_half_opacity(700.0, 0.25)?];
    let [lighter, heavier] = faded_faces.each_ref().map(Vec::as_slice);
    assert!(!has_colored_fringe(&between));
    assert!(alike(
        &between,
        &faces_in_shares(&black, [lighter, heavier], [1.0, 1.0])
    ));
    Ok(())
}
