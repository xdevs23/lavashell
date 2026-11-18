// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_scenes::{TEST_SURFACE, leaves, paint_damage};
use crate::test_surfaces::{Pixels, WIDE_SURFACE, pixels, surface};
use crate::{Error, Fonts, Painter, StreamFrames, SurfaceFrame, outline_path};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Packet;
use lavashell_core::surface::SurfaceOutline;
use std::time::Duration;

const LAYOUT: Viewport = Viewport::new(Size::new(Dp::new(40.0), Dp::new(30.0)), Density::ONE);

const MARK: Rect = Rect::new(Dp::new(10.0), Dp::new(5.0), Dp::new(10.0), Dp::new(10.0));

const OTHER_MARK: Rect = Rect::new(Dp::new(25.0), Dp::new(15.0), Dp::new(10.0), Dp::new(10.0));

const CLEAR: skia_safe::Color = skia_safe::Color::TRANSPARENT;

fn wide() -> Result<PixelScale, NumberError> {
    PixelScale::filling(LAYOUT, WIDE_SURFACE.pixel_size())
}

fn marked(marks: &[Rect], color: Color) -> Result<SceneSnapshot, TestError> {
    let fill = |mark: &Rect| DrawCommand::Fill {
        shape: (*mark).into(),
        brush: color.into(),
    };
    let packets = marks.iter().map(|mark| Packet::new([fill(mark)].into()));
    leaves(packets.collect::<Result<Vec<_>, _>>()?)
}

fn painter() -> Result<Painter, TestError> {
    Ok(Painter::without_device(&Fonts::new())?)
}

fn paint_wide(
    painter: &mut Painter,
    scene: &SceneSnapshot,
    damage: Rect,
) -> Result<Pixels<[u8; 4]>, TestError> {
    let mut target = surface(64, 30)?;
    let dp_to_output = wide()?.dp_to_output(LAYOUT.density);
    let canvas = target.canvas();
    paint_damage(
        painter,
        TEST_SURFACE,
        canvas,
        scene,
        dp_to_output,
        Some(damage),
        CLEAR,
    )?;
    Ok(pixels(&mut target))
}

#[test]
fn a_node_at_a_non_square_scale_is_drawn_at_the_scaled_pixels() -> Result<(), TestError> {
    let scene = marked(&[MARK], Color::WHITE)?;
    let drawn = paint_wide(&mut painter()?, &scene, Rect::from_size(LAYOUT.size))?;
    let alpha = |(x, y)| drawn.at(x, y)[3];
    let inside = [(17, 6), (30, 13)].map(alpha);
    let outside = [(14, 10), (33, 10), (24, 3), (24, 16)].map(alpha);
    assert_eq!((inside, outside), ([255; 2], [0; 4]));
    Ok(())
}

#[test]
fn the_damage_of_a_scaled_frame_repaints_only_the_scaled_pixels() -> Result<(), TestError> {
    let whole = Rect::from_size(LAYOUT.size);
    let white = marked(&[MARK, OTHER_MARK], Color::WHITE)?;
    let blue = marked(&[MARK, OTHER_MARK], Color::rgb(0x0020_50d0))?;
    let mut painter = painter()?;
    let before = paint_wide(&mut painter, &white, whole)?;
    let after = paint_wide(&mut painter, &blue, whole)?;
    let mut target = surface(64, 30)?;
    let dp_to_output = wide()?.dp_to_output(LAYOUT.density);
    for (scene, damage) in [(&white, whole), (&blue, MARK)] {
        let canvas = target.canvas();
        paint_damage(
            &mut painter,
            TEST_SURFACE,
            canvas,
            scene,
            dp_to_output,
            Some(damage),
            CLEAR,
        )?;
    }
    let shown = pixels(&mut target);
    assert_ne!(before.at(24, 10), after.at(24, 10));
    assert_eq!(shown.at(24, 10), after.at(24, 10));
    assert_ne!(before.at(48, 20), after.at(48, 20));
    assert_eq!(shown.at(48, 20), before.at(48, 20));
    Ok(())
}

#[test]
fn damage_beyond_the_scaled_buffer_repaints_nothing() -> Result<(), TestError> {
    let white = marked(&[MARK], Color::WHITE)?;
    let blue = marked(&[MARK], Color::rgb(0x0020_50d0))?;
    let beyond = Rect::new(Dp::new(45.0), Dp::ZERO, Dp::new(10.0), Dp::new(10.0));
    let mut painter = painter()?;
    let mut target = surface(64, 30)?;
    let dp_to_output = wide()?.dp_to_output(LAYOUT.density);
    let mut damaged_pixels = Vec::new();
    for (scene, damage) in [(&white, Rect::from_size(LAYOUT.size)), (&blue, beyond)] {
        let canvas = target.canvas();
        paint_damage(
            &mut painter,
            TEST_SURFACE,
            canvas,
            scene,
            dp_to_output,
            Some(damage),
            CLEAR,
        )?;
        damaged_pixels.push(painter.render_stats().damaged_pixels);
    }
    assert_eq!(damaged_pixels, [64 * 30; 2]);
    assert_eq!(pixels(&mut target).at(24, 10), [255; 4]);
    Ok(())
}

#[test]
fn an_outline_in_dp_clips_the_scaled_pixels() -> Result<(), TestError> {
    let band = Rect::new(Dp::new(5.0), Dp::ZERO, Dp::new(10.0), LAYOUT.size.height);
    let outline = SurfaceOutline::Rounded(RoundedRect::new(band, Corners::all(Dp::ZERO)));
    let shape = outline_path(&outline)?;
    let scene = marked(&[Rect::from_size(LAYOUT.size)], Color::WHITE)?;
    let frame = SurfaceFrame {
        surface: TEST_SURFACE,
        tree: scene.at(Duration::ZERO),
        dp_to_output: wide()?.dp_to_output(LAYOUT.density),
        damage: Some(Rect::from_size(LAYOUT.size)),
        clear: CLEAR,
        scene: None,
        shape: Some(&shape),
        streams: &StreamFrames::default(),
    };
    let mut target = surface(64, 30)?;
    painter()?.paint_surface(target.canvas(), frame)?;
    let drawn = pixels(&mut target);
    assert_eq!([7, 8, 23, 24].map(|x| drawn.at(x, 15)[3]), [0, 255, 255, 0]);
    Ok(())
}

#[test]
fn a_scale_beyond_the_range_of_skia_scalars_fails_the_frame() -> Result<(), TestError> {
    let speck = Viewport::new(Size::new(Dp::new(1e-300), Dp::new(1.0)), Density::ONE);
    let endless = PixelScale::filling(speck, WIDE_SURFACE.pixel_size())?;
    let mut target = surface(64, 30)?;
    let scene = marked(&[MARK], Color::WHITE)?;
    let dp_to_output = endless.dp_to_output(LAYOUT.density);
    let drawn = paint_damage(
        &mut painter()?,
        TEST_SURFACE,
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
