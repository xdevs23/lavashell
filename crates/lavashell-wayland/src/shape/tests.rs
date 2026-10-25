// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::prelude::{Brush, Color, Density, Dp, DpExt, DrawCommand, Rect, SceneTree};
use lavashell_renderer::skia::{Contains, IPoint};
use lavashell_renderer::{Fonts, Painter, SurfaceFrame, skia};
use num_traits::ToPrimitive;
use std::time::Duration;

fn shape() -> WindowShape {
    WindowShape::new(|size| {
        vec![
            vec![
                Point::new(3.2.dp(), 3.3.dp()),
                Point::new(size.width - 4.3.dp(), 3.3.dp()),
                Point::new(size.width - 4.3.dp(), size.height - 4.2.dp()),
                Point::new(size.width * 0.55, size.height - 4.2.dp()),
                Point::new(size.width * 0.55, size.height * 0.6),
                Point::new(3.2.dp(), size.height * 0.6),
            ],
            vec![
                Point::new(12.1.dp(), 12.2.dp()),
                Point::new(30.1.dp(), 12.2.dp()),
                Point::new(30.1.dp(), 28.2.dp()),
                Point::new(12.1.dp(), 28.2.dp()),
            ],
        ]
    })
}

/// The density with `scale` physical pixels for each dp.
pub(super) fn density_for(scale: f64) -> Result<Density, TestError> {
    Density::from_scale(scale).ok_or(TestError::Density(scale))
}

/// The nearest whole logical pixel of `length`.
fn whole_logical(length: Dp) -> Result<i32, TestError> {
    logical::whole(length).ok_or(TestError::OutOfRange(length.get()))
}

/// The pixels, truncated to whole ones, that `length` covers at `density`.
fn whole_pixels(density: Density, length: Dp) -> Result<i32, TestError> {
    let pixels = density.to_px(length).get();
    pixels.to_i32().ok_or(TestError::OutOfRange(pixels))
}

#[test]
fn input_cells_match_the_painted_even_odd_outline_after_resize_and_scaling() -> Result<(), TestError>
{
    for size in [Size::new(96.dp(), 64.dp()), Size::new(120.dp(), 90.dp())] {
        for scale in [1.0, 1.5, 2.0] {
            let centre = |cell: i16| scalar((f64::from(cell) + 0.5) * scale);
            let geometry = shape().compile(size, density_for(scale)?)?;
            let cells = |length: Dp| Ok::<_, TestError>(i16::try_from(whole_logical(length)?)?);
            for y in 0..cells(size.height)? {
                for x in 0..cells(size.width)? {
                    let input = takes_input(&geometry, i32::from(x), i32::from(y));
                    let painted = geometry.path.contains((centre(x)?, centre(y)?));
                    assert_eq!(input, painted, "{size:?}, scale {scale}, cell {x},{y}");
                }
            }
        }
    }
    Ok(())
}

/// Whether the native input region contains the logical cell at `x`, `y`.
pub(super) fn takes_input(geometry: &ShapeGeometry, x: i32, y: i32) -> bool {
    (geometry.input_cells.iter()).any(|cells| cells.contains(IPoint::new(x, y)))
}

/// The pixel of `surface` that contains the point at `x`, `y`.
fn pixel(
    surface: &mut skia::Surface,
    density: Density,
    x: Dp,
    y: Dp,
) -> Result<[u8; 4], TestError> {
    let info = skia::ImageInfo::new(
        (1, 1),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
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
        let info = skia::ImageInfo::new(
            physical,
            skia::ColorType::RGBA8888,
            skia::AlphaType::Premul,
            None,
        );
        let mut surface =
            skia::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let geometry = shape().compile(size, density)?;
        let mut painter = Painter::new(
            &Fonts::new(),
            lavashell_renderer::UniformRange::VULKAN_REQUIRED,
        )
        .map_err(Error::CreatePainter)?;
        let bounds = Rect::new(0.dp(), 0.dp(), size.width, size.height);
        let fill = DrawCommand::Fill {
            shape: bounds.into(),
            brush: Brush::Solid(Color::WHITE),
        };
        let tree = SceneTree::with_leaf([fill])?;
        let canvas = surface.canvas().clear(skia::Color::RED);
        painter
            .paint_surface(
                canvas,
                SurfaceFrame {
                    tree: tree.at(Duration::ZERO),
                    density,
                    damage: Some(bounds),
                    clear: skia::Color::TRANSPARENT,
                    scene: None,
                    shape: Some(&geometry.path),
                },
            )
            .map_err(Error::Paint)?;
        assert_eq!(pixel(&mut surface, density, 0.dp(), 0.dp())?, [0; 4]);
        assert_eq!(pixel(&mut surface, density, 20.dp(), 20.dp())?, [0; 4]);
        assert_eq!(pixel(&mut surface, density, 45.dp(), 10.dp())?, [255; 4]);
        let edge = pixel(&mut surface, density, 3.dp(), 10.dp())?;
        assert!(
            edge[3] > 0 && edge[3] < 255,
            "expected fractional edge coverage: {edge:?}"
        );
        for _ in 0..2 {
            painter
                .paint_surface(
                    surface.canvas(),
                    SurfaceFrame {
                        tree: tree.at(Duration::ZERO),
                        density,
                        damage: Some(Rect::new(0.dp(), 0.dp(), 48.dp(), 64.dp())),
                        clear: skia::Color::TRANSPARENT,
                        scene: None,
                        shape: Some(&geometry.path),
                    },
                )
                .map_err(Error::Paint)?;
            assert_eq!(pixel(&mut surface, density, 3.dp(), 10.dp())?, edge);
        }
    }
    Ok(())
}

#[test]
fn invalid_contours_fail_and_an_empty_outline_is_fully_click_through() -> Result<(), TestError> {
    let size = Size::new(96.dp(), 64.dp());
    for contour in [
        vec![Point::default()],
        vec![Point::new(Dp::new(f64::NAN), 1.dp()); 3],
    ] {
        assert!(
            WindowShape::new(move |_| vec![contour.clone()])
                .compile(size, Density::ONE)
                .is_err()
        );
    }
    let empty = WindowShape::new(|_| Vec::new()).compile(size, Density::ONE)?;
    assert!(empty.input_cells.is_empty());
    assert!(!empty.path.contains((12.0, 12.0)));
    Ok(())
}

#[test]
fn input_passthrough_retains_the_visual_and_background_effect_outline() -> Result<(), TestError> {
    let shape = WindowShape::new(|_| {
        vec![vec![
            Point::new(4.dp(), 4.dp()),
            Point::new(28.dp(), 4.dp()),
            Point::new(28.dp(), 20.dp()),
            Point::new(4.dp(), 20.dp()),
        ]]
    })
    .input_passthrough();
    let geometry = shape.compile(Size::new(32.dp(), 24.dp()), Density::ONE)?;
    assert!(shape.clip_paint);
    assert!(!shape.accept_input);
    assert!(!geometry.input_cells.is_empty());
    assert!(geometry.path.contains((16.0, 12.0)));
    assert!(!geometry.path.contains((0.0, 0.0)));
    Ok(())
}
