// SPDX-License-Identifier: GPL-3.0-only
use crate::BlendSpace;
use crate::failure::DrawFailure;
use crate::scalar::scalars;
use crate::test_error::TestError;
use crate::test_font::{probe_style, probe_style_before_scaling};
use crate::test_surfaces::{
    Pixels, drawn_pixels, drawn_surface, float_info, float_pixels, has_colored_fringe,
    lcd_destination,
};
use crate::text::sample_fixtures::{LABEL_SURFACE, PLAIN_LABEL_TEXT};
use crate::text::weight_blend_fixtures::{
    BETWEEN_WEIGHT, HEAVIER_WEIGHT, LIGHTER_WEIGHT, ONE_LINE, faces_in_shares, largest_deviation,
    shares_between,
};
use crate::text::{TextAntialiasing, TextRendering};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType};

fn static_face_painter() -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::static_test_faces()?;
    Ok(crate::Painter::without_device(&fonts)?)
}

fn srgb_painter() -> Result<crate::Painter, TestError> {
    let mut painter = static_face_painter()?;
    painter.set_blend_space(BlendSpace::Srgb);
    Ok(painter)
}

fn lcd_painter() -> Result<crate::Painter, TestError> {
    let mut painter = static_face_painter()?;
    let subpixel = TextRendering {
        antialiasing: TextAntialiasing::Subpixel,
        ..Default::default()
    };
    painter.set_text_rendering(subpixel, lcd_destination());
    Ok(painter)
}

fn text_in_weight(rect: Rect, weight: f64, color: Color) -> Result<DrawCommand, TestError> {
    Ok(DrawCommand::Text {
        text: PLAIN_LABEL_TEXT.into(),
        rect,
        style: probe_style_before_scaling()
            .color(color)
            .weight(FontWeight::try_new(weight)?)
            .resolve_unscaled(),
    })
}

fn extended_srgb_pixels(
    painter: &mut crate::Painter,
    commands: &[DrawCommand],
) -> Result<Pixels<[f32; 4]>, TestError> {
    let info = float_info(
        LABEL_SURFACE.dimensions(),
        AlphaType::Premul,
        Some(ColorSpace::new_srgb()),
    );
    let mut surface = drawn_surface(&info, None, skia_safe::Color::TRANSPARENT, |canvas| {
        painter.paint_as_leaf(canvas, commands, Density::ONE)
    })?;
    Ok(float_pixels(
        &mut surface,
        AlphaType::Premul,
        Some(ColorSpace::new_srgb()),
    ))
}

fn on_lcd_canvas_half_a_pixel_off(
    painter: &mut crate::Painter,
    backdrop: skia_safe::Color,
    commands: &[DrawCommand],
) -> Result<Pixels<[u8; 4]>, TestError> {
    let props = lcd_destination().surface_props();
    drawn_pixels(
        LABEL_SURFACE,
        ColorType::BGRA8888,
        AlphaType::Opaque,
        Some(&props),
        backdrop,
        |canvas| {
            let _canvas_returned_for_chaining = canvas.translate((0.5, 0.0));
            painter.paint_as_leaf(canvas, commands, Density::ONE)
        },
    )
}

fn faded_faces_in_extended_srgb(rect: Rect, color: Color) -> Result<Pixels<[f64; 4]>, TestError> {
    let mut reference = srgb_painter()?;
    let [lighter_share, heavier_share] = shares_between();
    let mut faded = |weight, share| {
        let faded_color = color.with_alpha(color.alpha() * share);
        extended_srgb_pixels(
            &mut reference,
            &[text_in_weight(rect, weight, faded_color)?],
        )
    };
    let lighter = faded(LIGHTER_WEIGHT, lighter_share)?;
    let heavier = faded(HEAVIER_WEIGHT, heavier_share)?;
    let transparent = lighter.map(|_| [0.0; 4]);
    faces_in_shares(&transparent, [&lighter, &heavier], [1.0, 1.0])
}

fn rounded_to_f32(pixels: &Pixels<[f64; 4]>) -> Result<Pixels<[f32; 4]>, TestError> {
    Ok(pixels.try_map(|pixel| scalars(*pixel))?)
}

#[test]
fn a_weight_blend_keeps_a_display_p3_color_beyond_srgb_while_blending_in_srgb()
-> Result<(), TestError> {
    let green = Color::encoded([0.0, 1.0, 0.0], 1.0, Gamut::DisplayP3);
    let both_faces = faded_faces_in_extended_srgb(ONE_LINE, green)?;
    assert!(both_faces.channels().iter().any(|channel| *channel > 1.0));
    let between = text_in_weight(ONE_LINE, BETWEEN_WEIGHT, green)?;
    assert_eq!(
        extended_srgb_pixels(&mut srgb_painter()?, &[between])?,
        rounded_to_f32(&both_faces)?
    );
    Ok(())
}

#[test]
fn a_weight_blend_paints_the_lines_of_a_paragraph_that_overflow_its_box() -> Result<(), TestError> {
    let short_box = Rect::new(7.dp(), 2.dp(), 60.dp(), 4.dp());
    let both_faces = faded_faces_in_extended_srgb(short_box, Color::WHITE)?;
    let lowest_inked_row = (both_faces.rows()?)
        .rposition(|row| row.iter().flatten().any(|channel| *channel > 0.0))
        .ok_or(TestError::Missing("ink of the paragraph"))?;
    let box_widened_by_the_font_size = short_box.outset(probe_style().size);
    let widened_bottom = box_widened_by_the_font_size.y + box_widened_by_the_font_size.height;
    assert!(f64::from(u32::try_from(lowest_inked_row)?) > widened_bottom.get());
    let between = text_in_weight(short_box, BETWEEN_WEIGHT, Color::WHITE)?;
    assert_eq!(
        extended_srgb_pixels(&mut srgb_painter()?, &[between])?,
        rounded_to_f32(&both_faces)?
    );
    Ok(())
}

#[test]
fn a_weight_blend_scaled_to_nothing_draws_nothing_and_records_no_failure() -> Result<(), TestError>
{
    let mut painter = srgb_painter()?;
    let scaled_to_nothing = [
        DrawCommand::Transform(Affine::scale(0.0, 0.0)),
        text_in_weight(ONE_LINE, BETWEEN_WEIGHT, Color::WHITE)?,
    ];
    let drawn = extended_srgb_pixels(&mut painter, &scaled_to_nothing)?;
    assert!(drawn.channels().iter().all(|channel| *channel == 0.0));
    assert_eq!(painter.failures.count(DrawFailure::SingularTransform), 0);
    Ok(())
}

const ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS: f64 = 2.0;

const LIGHT_BACKDROP: skia_safe::Color = skia_safe::Color::from_rgb(236, 232, 224);

#[test]
fn lcd_text_that_skia_draws_at_rest_stays_lcd_through_a_weight_blend() -> Result<(), TestError> {
    let mut painter = lcd_painter()?;
    for weight in [LIGHTER_WEIGHT, BETWEEN_WEIGHT, HEAVIER_WEIGHT] {
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
        let lighter = drawn(&[text_in_weight(ONE_LINE, LIGHTER_WEIGHT, color)?])?;
        let heavier = drawn(&[text_in_weight(ONE_LINE, HEAVIER_WEIGHT, color)?])?;
        let between = drawn(&[text_in_weight(ONE_LINE, BETWEEN_WEIGHT, color)?])?;
        let one_weight_off = drawn(&[text_in_weight(ONE_LINE, LIGHTER_WEIGHT + 1.0, color)?])?;
        let faces = [&lighter, &heavier];
        let blended = faces_in_shares(&empty, faces, shares_between())?;
        assert!(
            largest_deviation(&between, &blended)? <= ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS
        );
        let lighter_alone = faces_in_shares(&empty, faces, [1.0, 0.0])?;
        assert!(
            largest_deviation(&one_weight_off, &lighter_alone)?
                <= ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS
        );
    }
    Ok(())
}

#[test]
fn a_weight_blend_in_an_opacity_group_on_an_lcd_canvas_adds_both_faces_in_grayscale()
-> Result<(), TestError> {
    let mut painter = lcd_painter()?;
    let black = on_lcd_canvas_half_a_pixel_off(&mut painter, skia_safe::Color::BLACK, &[])?;
    let mut in_half_opacity = |weight, alpha| -> Result<Pixels<[u8; 4]>, TestError> {
        let commands = [
            DrawCommand::Opacity(0.5),
            text_in_weight(ONE_LINE, weight, Color::WHITE.with_alpha(alpha))?,
            DrawCommand::Restore,
        ];
        on_lcd_canvas_half_a_pixel_off(&mut painter, skia_safe::Color::BLACK, &commands)
    };
    let [lighter_share, heavier_share] = shares_between();
    let between = in_half_opacity(BETWEEN_WEIGHT, 1.0)?;
    let lighter = in_half_opacity(LIGHTER_WEIGHT, lighter_share)?;
    let heavier = in_half_opacity(HEAVIER_WEIGHT, heavier_share)?;
    assert!(!has_colored_fringe(&between));
    let both_faces = faces_in_shares(&black, [&lighter, &heavier], [1.0, 1.0])?;
    assert!(
        largest_deviation(&between, &both_faces)? <= ROUNDING_STEPS_OF_LAYERED_AND_SEPARATE_DRAWS
    );
    Ok(())
}
