// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_scenes::{leaves, paint_damage};
use crate::test_surfaces::{pixels, surface};
use crate::{Error, Fonts, Painter, StreamFrames, SurfaceFrame, outline_path};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Packet;
use lavashell_core::surface::SurfaceOutline;
use std::time::Duration;

const LAYOUT: Viewport = Viewport::new(Size::new(Dp::new(40.0), Dp::new(30.0)), Density::ONE);

/// Spans 16 to 32 output pixels across and 5 to 15 down at the scale returned by `wide`.
const MARK: Rect = Rect::new(Dp::new(10.0), Dp::new(5.0), Dp::new(10.0), Dp::new(10.0));

/// Spans 40 to 56 output pixels across and 15 to 25 down at the scale returned by `wide`.
const OTHER_MARK: Rect = Rect::new(Dp::new(25.0), Dp::new(15.0), Dp::new(10.0), Dp::new(10.0));

const CLEAR: skia_safe::Color = skia_safe::Color::TRANSPARENT;

/// Covers 1.6 output pixels across and 1 down for each pixel of `LAYOUT`.
fn wide() -> Result<PixelScale, NumberError> {
    PixelScale::filling(LAYOUT, PixelSize::new(64, 30))
}

/// A scene with one leaf for each of `marks`, filled with `color`.
fn marked(marks: &[Rect], color: Color) -> Result<SceneSnapshot, TestError> {
    leaves(marks.iter().map(|mark| {
        let fill = DrawCommand::Fill {
            shape: (*mark).into(),
            brush: color.into(),
        };
        Packet::new([fill].into())
    }))
}

/// The pixel at `x`, `y` of a target 64 pixels wide.
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 64 + x) * 4..][..4]
}

#[test]
fn a_node_at_a_non_square_scale_is_drawn_at_the_scaled_pixels() -> Result<(), TestError> {
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = surface(64, 30)?;
    let dp_to_output = wide()?.dp_to_output(LAYOUT.density);
    let whole = Some(Rect::from_size(LAYOUT.size));
    let scene = marked(&[MARK], Color::WHITE)?;
    paint_damage(
        &mut painter,
        target.canvas(),
        &scene,
        dp_to_output,
        whole,
        CLEAR,
    )?;
    let drawn = pixels(&mut target);
    let at = |(x, y)| pixel(&drawn, x, y)[3];
    let inside = [(17, 6), (30, 13)].map(at);
    let outside = [(14, 10), (33, 10), (24, 3), (24, 16)].map(at);
    assert_eq!((inside, outside), ([255; 2], [0; 4]));
    Ok(())
}

#[test]
fn the_damage_of_a_scaled_frame_repaints_only_the_scaled_pixels() -> Result<(), TestError> {
    let dp_to_output = wide()?.dp_to_output(LAYOUT.density);
    let whole = Some(Rect::from_size(LAYOUT.size));
    let white = marked(&[MARK, OTHER_MARK], Color::WHITE)?;
    let blue = marked(&[MARK, OTHER_MARK], Color::rgb(0x0020_50d0))?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut whole_frame = |scene: &SceneSnapshot| -> Result<Vec<u8>, TestError> {
        let mut target = surface(64, 30)?;
        paint_damage(
            &mut painter,
            target.canvas(),
            scene,
            dp_to_output,
            whole,
            CLEAR,
        )?;
        Ok(pixels(&mut target))
    };
    let (before, after) = (whole_frame(&white)?, whole_frame(&blue)?);
    let mut target = surface(64, 30)?;
    paint_damage(
        &mut painter,
        target.canvas(),
        &white,
        dp_to_output,
        whole,
        CLEAR,
    )?;
    paint_damage(
        &mut painter,
        target.canvas(),
        &blue,
        dp_to_output,
        Some(MARK),
        CLEAR,
    )?;
    let shown = pixels(&mut target);
    assert_ne!(pixel(&before, 24, 10), pixel(&after, 24, 10));
    assert_eq!(pixel(&shown, 24, 10), pixel(&after, 24, 10));
    assert_ne!(pixel(&before, 48, 20), pixel(&after, 48, 20));
    assert_eq!(pixel(&shown, 48, 20), pixel(&before, 48, 20));
    Ok(())
}

#[test]
fn an_outline_in_dp_clips_the_scaled_pixels() -> Result<(), TestError> {
    let band = Rect::new(Dp::new(5.0), Dp::ZERO, Dp::new(10.0), LAYOUT.size.height);
    let outline = SurfaceOutline::Rounded(RoundedRect::new(band, Corners::all(Dp::ZERO)));
    let shape = outline_path(&outline)?;
    let scene = marked(&[Rect::from_size(LAYOUT.size)], Color::WHITE)?;
    let frame = SurfaceFrame {
        tree: scene.at(Duration::ZERO),
        dp_to_output: wide()?.dp_to_output(LAYOUT.density),
        damage: Some(Rect::from_size(LAYOUT.size)),
        clear: CLEAR,
        scene: None,
        shape: Some(&shape),
        streams: &StreamFrames::default(),
    };
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = surface(64, 30)?;
    painter.paint_surface(target.canvas(), frame)?;
    let drawn = pixels(&mut target);
    let at = |x| pixel(&drawn, x, 15)[3];
    assert_eq!([7, 8, 23, 24].map(at), [0, 255, 255, 0]);
    Ok(())
}

#[test]
fn a_scale_beyond_the_range_of_skia_scalars_fails_the_frame() -> Result<(), TestError> {
    let speck = Viewport::new(Size::new(Dp::new(1e-300), Dp::new(1.0)), Density::ONE);
    let endless = PixelScale::filling(speck, PixelSize::new(64, 30))?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut target = surface(64, 30)?;
    let scene = marked(&[MARK], Color::WHITE)?;
    let dp_to_output = endless.dp_to_output(LAYOUT.density);
    let drawn = paint_damage(
        &mut painter,
        target.canvas(),
        &scene,
        dp_to_output,
        None,
        CLEAR,
    );
    assert!(matches!(
        drawn,
        Err(TestError::Renderer(Error::OutputTransform(_)))
    ));
    Ok(())
}
