// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::{Fonts, Painter, SurfaceFrame, UniformRange, input_cells};
use lavashell_core::geometry::{Point, Size};
use lavashell_core::prelude::{Brush, Color, Corners, DpExt, DrawCommand, Rect, SceneTree};
use num_traits::ToPrimitive;
use skia_safe::{Contains, IPoint, IRect};
use std::time::Duration;

fn outline(size: Size) -> Result<SurfaceOutline, TestError> {
    Ok(SurfaceOutline::Contours(vec![
        Polygon::new([
            Point::new(3.2.dp(), 3.3.dp()),
            Point::new(size.width - 4.3.dp(), 3.3.dp()),
            Point::new(size.width - 4.3.dp(), size.height - 4.2.dp()),
            Point::new(size.width * 0.55, size.height - 4.2.dp()),
            Point::new(size.width * 0.55, size.height * 0.6),
            Point::new(3.2.dp(), size.height * 0.6),
        ])?,
        Polygon::new([
            Point::new(12.1.dp(), 12.2.dp()),
            Point::new(30.1.dp(), 12.2.dp()),
            Point::new(30.1.dp(), 28.2.dp()),
            Point::new(12.1.dp(), 28.2.dp()),
        ])?,
    ]))
}

/// The clip path in physical pixels and the input cells of `outline` on a surface of `size` at `density`.
fn geometry(
    outline: &SurfaceOutline,
    size: Size,
    density: Density,
) -> Result<(Path, Vec<IRect>), TestError> {
    let path = outline_path(outline)?;
    Ok((clip_path(&path, density)?, input_cells(&path, size)?))
}

/// The density with `scale` physical pixels for each dp.
fn density_for(scale: f64) -> Result<Density, TestError> {
    Density::from_scale(scale).ok_or(TestError::NoDensity(scale))
}

/// The number of logical cells along `length`, which is a whole number of dp.
fn cells(length: Dp) -> Result<i16, TestError> {
    length
        .get()
        .to_i16()
        .ok_or(TestError::Missing("a cell count"))
}

/// The pixels, truncated to whole ones, that `length` covers at `density`.
fn whole_pixels(density: Density, length: Dp) -> Result<i32, TestError> {
    (density.to_px(length).get().to_i32()).ok_or(TestError::Missing("a pixel count"))
}

/// Whether `input_cells` contain the logical cell at `x`, `y`.
fn takes_input(input_cells: &[IRect], x: i16, y: i16) -> bool {
    let cell = IPoint::new(i32::from(x), i32::from(y));
    (input_cells.iter()).any(|cells| cells.contains(cell))
}

/// The physical position of the center of the logical cell `cell` at `scale`.
fn center(cell: i16, scale: f64) -> Result<f32, TestError> {
    Ok(scalar((f64::from(cell) + 0.5) * scale)?)
}

#[test]
fn input_cells_match_the_painted_even_odd_outline_after_resize_and_scaling() -> Result<(), TestError>
{
    for size in [Size::new(96.dp(), 64.dp()), Size::new(120.dp(), 90.dp())] {
        for scale in [1.0, 1.5, 2.0] {
            let (clip, input_cells) = geometry(&outline(size)?, size, density_for(scale)?)?;
            for y in 0..cells(size.height)? {
                for x in 0..cells(size.width)? {
                    let input = takes_input(&input_cells, x, y);
                    let painted = clip.contains((center(x, scale)?, center(y, scale)?));
                    assert_eq!(input, painted, "{size:?}, scale {scale}, cell {x},{y}");
                }
            }
        }
    }
    Ok(())
}

/// The pixel of `surface` that contains the point at `x`, `y`.
fn pixel(
    surface: &mut skia_safe::Surface,
    density: Density,
    x: Dp,
    y: Dp,
) -> Result<[u8; 4], TestError> {
    let info = skia_safe::ImageInfo::new(
        (1, 1),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut bytes = [0; 4];
    let origin = (whole_pixels(density, x)?, whole_pixels(density, y)?);
    assert!(surface.read_pixels(&info, &mut bytes, 4, origin));
    Ok(bytes)
}

#[test]
fn damaged_paint_clears_holes_and_reconstructs_antialiased_edges_without_accumulation()
-> Result<(), TestError> {
    let size = Size::new(96.dp(), 64.dp());
    for scale in [1.0, 1.5, 2.0] {
        let density = density_for(scale)?;
        let physical = (
            whole_pixels(density, size.width)?,
            whole_pixels(density, size.height)?,
        );
        let info = skia_safe::ImageInfo::new(
            physical,
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let mut surface =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let clip = clip_path(&outline_path(&outline(size)?)?, density)?;
        let mut painter = Painter::new(&Fonts::new(), UniformRange::VULKAN_REQUIRED)?;
        let bounds = Rect::new(0.dp(), 0.dp(), size.width, size.height);
        let fill = DrawCommand::Fill {
            shape: bounds.into(),
            brush: Brush::Solid(Color::WHITE),
        };
        let tree = SceneTree::with_leaf([fill])?;
        let canvas = surface.canvas().clear(skia_safe::Color::RED);
        painter.paint_surface(
            canvas,
            SurfaceFrame {
                tree: tree.at(Duration::ZERO),
                density,
                damage: Some(bounds),
                clear: skia_safe::Color::TRANSPARENT,
                scene: None,
                shape: Some(&clip),
            },
        )?;
        assert_eq!(pixel(&mut surface, density, 0.dp(), 0.dp())?, [0; 4]);
        assert_eq!(pixel(&mut surface, density, 20.dp(), 20.dp())?, [0; 4]);
        assert_eq!(pixel(&mut surface, density, 45.dp(), 10.dp())?, [255; 4]);
        let edge = pixel(&mut surface, density, 3.dp(), 10.dp())?;
        assert!(
            edge[3] > 0 && edge[3] < 255,
            "expected fractional edge coverage: {edge:?}"
        );
        for _ in 0..2 {
            painter.paint_surface(
                surface.canvas(),
                SurfaceFrame {
                    tree: tree.at(Duration::ZERO),
                    density,
                    damage: Some(Rect::new(0.dp(), 0.dp(), 48.dp(), 64.dp())),
                    clear: skia_safe::Color::TRANSPARENT,
                    scene: None,
                    shape: Some(&clip),
                },
            )?;
            assert_eq!(pixel(&mut surface, density, 3.dp(), 10.dp())?, edge);
        }
    }
    Ok(())
}

#[test]
fn an_empty_outline_takes_no_input_and_clips_everything() -> Result<(), TestError> {
    let size = Size::new(96.dp(), 64.dp());
    let (clip, input_cells) = geometry(&SurfaceOutline::Empty, size, Density::ONE)?;
    assert!(input_cells.is_empty());
    assert!(!clip.contains((12.0, 12.0)));
    Ok(())
}

#[test]
fn an_outline_beyond_the_range_of_skia_scalars_once_scaled_fails() -> Result<(), TestError> {
    let far = Dp::new(f64::from(f32::MAX));
    let triangle = Polygon::new([
        Point::default(),
        Point::new(far, 0.dp()),
        Point::new(0.dp(), far),
    ])?;
    let path = outline_path(&SurfaceOutline::Contours(vec![triangle]))?;
    assert!(matches!(
        clip_path(&path, density_for(2.0)?),
        Err(OutlineError::Scaled(_))
    ));
    Ok(())
}

fn rounded_outline(rect: Rect, radius: Dp) -> SurfaceOutline {
    SurfaceOutline::Rounded(RoundedRect::new(rect, Corners::all(radius)))
}

#[test]
fn fractional_rounded_regions_exclude_transparent_corners_for_paint_and_input()
-> Result<(), TestError> {
    for phase in [0.0, 0.25, 0.75] {
        let rect = Rect::new(Dp::new(phase), Dp::new(phase), 320.dp(), 124.dp());
        let outline = rounded_outline(rect, 14.dp());
        for scale in [1.0, 1.5, 2.0] {
            let size = Size::new(321.dp(), 125.dp());
            let (clip, input_cells) = geometry(&outline, size, density_for(scale)?)?;
            for (x, y, expected) in [
                (1, 1, false),
                (160, 62, true),
                (318, 1, false),
                (318, 122, false),
            ] {
                let input = takes_input(&input_cells, x, y);
                let painted = clip.contains((center(x, scale)?, center(y, scale)?));
                assert_eq!(input, expected, "phase {phase} cell {x},{y}");
                assert_eq!(painted, input);
            }
        }
    }
    Ok(())
}

#[test]
fn degenerate_rectangles_are_empty_but_nonfinite_rounded_outlines_fail() -> Result<(), TestError> {
    let size = Size::new(10.dp(), 10.dp());
    let degenerate = rounded_outline(Rect::default(), 14.dp());
    let (_clip, input_cells) = geometry(&degenerate, size, Density::ONE)?;
    assert!(input_cells.is_empty());
    let rect = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
    let nonfinite = rounded_outline(rect, Dp::new(f64::NAN));
    assert!(matches!(
        outline_path(&nonfinite),
        Err(OutlineError::Rounded(_))
    ));
    Ok(())
}
