// SPDX-License-Identifier: GPL-3.0-only
use super::{source, within_largest_image};
use crate::failure::DrawFailure;
use crate::test_error::TestError;
use crate::test_surfaces::{pattern, pixels, surface};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Packet;
use skia_safe::canvas::SrcRectConstraint;
use skia_safe::{Contains, FilterMode, ISize, MipmapMode, Paint, Rect as SkRect, SamplingOptions};
use std::sync::Arc;

const SIDE: i32 = 72;

#[test]
fn an_image_decodes_while_it_has_no_more_pixels_than_the_largest_square_image() {
    assert!(within_largest_image((4096, 4096), 4096));
    assert!(within_largest_image((16384, 1024), 4096));
    assert!(!within_largest_image((4097, 4096), 4096));
    assert!(!within_largest_image((u32::MAX, u32::MAX), i32::MAX));
}

#[test]
fn a_frame_on_the_box_uses_the_whole_image() {
    let wide = Rect::new(0.dp(), 0.dp(), 100.dp(), 50.dp());
    assert_eq!(
        source(wide, wide.size(), ISize::new(400, 200)),
        Ok(Some(SkRect::from_wh(400.0, 200.0)))
    );
}

#[test]
fn a_frame_larger_than_the_box_crops_the_image_by_the_image_pixels_in_one_dp() {
    // The frame shows two image pixels in one dp.
    let zoomed = Rect::new((-50).dp(), (-25).dp(), 200.dp(), 100.dp());
    assert_eq!(
        source(zoomed, Size::new(100.dp(), 50.dp()), ISize::new(400, 200)),
        Ok(Some(SkRect::from_xywh(100.0, 50.0, 200.0, 100.0)))
    );
}

#[test]
fn a_frame_smaller_than_the_box_leaves_the_box_extending_past_the_image() {
    let small = Rect::new(25.dp(), 25.dp(), 50.dp(), 50.dp());
    assert_eq!(
        source(small, Size::new(100.dp(), 100.dp()), ISize::new(100, 100)),
        Ok(Some(SkRect::from_xywh(-50.0, -50.0, 200.0, 200.0)))
    );
}

#[test]
fn a_frame_with_a_zero_or_infinite_size_has_no_source() {
    let size = Size::new(100.dp(), 100.dp());
    let flat = Rect::new(0.dp(), 0.dp(), 0.dp(), 100.dp());
    assert_eq!(source(flat, size, ISize::new(100, 100)), Ok(None));
    let endless = Rect::new(0.dp(), 0.dp(), 100.dp(), Dp::new(f64::INFINITY));
    assert_eq!(source(endless, size, ISize::new(100, 100)), Ok(None));
}

fn image(decoded: &skia_safe::Image) -> Result<Image, TestError> {
    let (width, height) = (
        u32::try_from(decoded.width())?,
        u32::try_from(decoded.height())?,
    );
    Ok(Image::encoded(Vec::new(), width, height).with_backend_data(decoded.clone()))
}

fn commands(image: &Image, destination: Rect, fit: ImageFit) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    let bounds = Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp());
    let context = DrawContext::new(Viewport::new(bounds.size(), Density::ONE));
    Drawing::new(&mut commands, bounds, &context).image(image, destination, fit);
    commands
}

fn render(commands: &[DrawCommand]) -> Result<Vec<u8>, TestError> {
    let mut surface = surface(SIDE, SIDE)?;
    Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?.paint_as_leaf(
        surface.canvas(),
        commands,
        Density::ONE,
    )?;
    Ok(pixels(&mut surface))
}

fn assert_opaque_inside(bytes: &[u8], drawn: SkRect) -> Result<(), TestError> {
    let width = usize::try_from(SIDE)?;
    for (index, pixel) in bytes.chunks(4).enumerate() {
        let (x, y) = (u16::try_from(index % width)?, u16::try_from(index / width)?);
        let center = skia_safe::Point::new(f32::from(x) + 0.5, f32::from(y) + 0.5);
        let inside = drawn.contains(center);
        assert_eq!(pixel[3], if inside { 255 } else { 0 }, "({x}, {y})");
    }
    Ok(())
}

#[test]
fn cover_draws_the_pixels_of_a_centered_crop_computed_by_hand() -> Result<(), TestError> {
    let decoded = pattern(70, 50)?;
    let destination = Rect::new(3.dp(), 5.dp(), 64.dp(), 64.dp());
    let covered = render(&commands(&image(&decoded)?, destination, ImageFit::cover()))?;
    // The box scales the image by 64 / 50, so a centered 50 × 50 square of the image fills it.
    let mut reference = surface(SIDE, SIDE)?;
    let _chained_canvas = reference.canvas().draw_image_rect_with_sampling_options(
        &decoded,
        Some((
            &SkRect::from_xywh(10.0, 0.0, 50.0, 50.0),
            SrcRectConstraint::Strict,
        )),
        SkRect::from_xywh(3.0, 5.0, 64.0, 64.0),
        SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        &Paint::default(),
    );
    assert_eq!(covered, pixels(&mut reference));
    Ok(())
}

#[test]
fn contain_leaves_the_rest_of_the_box_empty() -> Result<(), TestError> {
    let destination = Rect::new(4.dp(), 4.dp(), 64.dp(), 64.dp());
    let image = image(&pattern(64, 32)?)?;
    let contained = render(&commands(&image, destination, ImageFit::contain()))?;
    assert_opaque_inside(&contained, SkRect::from_xywh(4.0, 20.0, 64.0, 32.0))
}

#[test]
fn a_frame_draws_only_inside_the_box() -> Result<(), TestError> {
    let destination = Rect::new(16.dp(), 16.dp(), 32.dp(), 32.dp());
    let image = image(&pattern(70, 50)?)?;
    let zoomed = ImageFrame::new(-50.pct(), -50.pct(), 200.pct(), 200.pct());
    let drawn = render(&commands(&image, destination, ImageFit::Frame(zoomed)))?;
    assert_opaque_inside(&drawn, SkRect::from_xywh(16.0, 16.0, 32.0, 32.0))?;
    let small = ImageFrame::new(8.dp(), 4.dp(), 16.dp(), 12.dp());
    let drawn = render(&commands(&image, destination, ImageFit::Frame(small)))?;
    assert_opaque_inside(&drawn, SkRect::from_xywh(24.0, 20.0, 16.0, 12.0))
}

#[test]
fn a_frame_or_an_image_without_area_draws_nothing() -> Result<(), TestError> {
    let decoded = pattern(70, 50)?;
    let destination = Rect::new(8.dp(), 8.dp(), 48.dp(), 48.dp());
    let flat = ImageFrame::new(0.dp(), 0.dp(), 0.dp(), 48.dp());
    let drawn = render(&commands(
        &image(&decoded)?,
        destination,
        ImageFit::Frame(flat),
    ))?;
    assert_opaque_inside(&drawn, SkRect::new_empty())?;
    let unmeasured = Image::encoded(Vec::new(), 0, 0).with_backend_data(decoded);
    let drawn = render(&commands(&unmeasured, destination, ImageFit::cover()))?;
    assert_opaque_inside(&drawn, SkRect::new_empty())
}

#[test]
fn an_image_that_declares_another_size_than_its_pixels_is_left_out() -> Result<(), TestError> {
    let declared = Image::encoded(Vec::new(), 35, 25).with_backend_data(pattern(70, 50)?);
    let destination = Rect::new(3.dp(), 5.dp(), 64.dp(), 64.dp());
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = surface(SIDE, SIDE)?;
    let commands = commands(&declared, destination, ImageFit::cover());
    painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
    assert_eq!(painter.failures.count(DrawFailure::ImageSize), 1);
    assert!(pixels(&mut target).chunks(4).all(|pixel| pixel[3] == 0));
    Ok(())
}

#[test]
fn an_image_cropped_between_pixels_on_a_cached_card_is_the_image_drawn_directly()
-> Result<(), TestError> {
    let image = image(&pattern(70, 50)?)?;
    let destination = Rect::new(8.dp(), 8.dp(), 48.dp(), 48.dp());
    let fit = ImageFit::Cover(ImagePlacement::at(Anchor::Top).offset(0.vw(), 10.vh()));
    let card = DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp()).into(),
        brush: Color::rgb(0x0018_2030).into(),
    };
    let card = [vec![card], commands(&image, destination, fit)].concat();
    let direct = render(&card)?;
    let tree = crate::test_scenes::leaves([Packet::new(Arc::from(card))])?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut cached = surface(SIDE, SIDE)?;
    for _ in 0..3 {
        let _chained_canvas = cached.canvas().clear(skia_safe::Color::TRANSPARENT);
        crate::test_scenes::paint(&mut painter, cached.canvas(), &tree, Density::ONE);
    }
    assert!(painter.render_stats().cache_hits > 0);
    assert_eq!(pixels(&mut cached), direct);
    Ok(())
}
