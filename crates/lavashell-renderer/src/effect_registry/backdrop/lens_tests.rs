// SPDX-License-Identifier: GPL-3.0-only
use super::cache_checks::background;
use super::*;
use crate::destination::Destination;
use crate::effect_checks::stripes;
use crate::effect_registry::SurfaceAccess;
use crate::failure::{DrawFailure, Failures};
use crate::field::test_shapes::{Distance, ellipse, union};
use crate::resources::PaintResources;
use crate::test_error::TestError;
use crate::test_surfaces::{EFFECT_SURFACE, Pixels, pixels};
use crate::{FieldRoom, Fonts, Painter, UniformRange};

const PAST_REQUIRED_COLUMNS: u16 = 16384 / 16 + 1;

fn shapes() -> Result<[Shape; 5], TestError> {
    let pill = |x| {
        RoundedRect::new(
            Rect::new(x, 8.dp(), 40.dp(), 32.dp()),
            Corners::all(16.dp()),
        )
        .into()
    };
    Ok([
        RoundedRect::new(
            Rect::new(10.dp(), 6.dp(), 76.dp(), 36.dp()),
            Corners::new(18.dp(), 0.dp(), 6.dp(), 12.dp()),
        )
        .into(),
        union([pill(6.dp()), pill(50.dp())], 12.dp())?,
        ellipse(12, Rect::new(12.dp(), 6.dp(), 72.dp(), 36.dp()))?,
        ellipse(
            PAST_REQUIRED_COLUMNS,
            Rect::new(12.dp(), 6.dp(), 72.dp(), 36.dp()),
        )?,
        union(
            (0..=PAST_REQUIRED_COLUMNS / 2).map(|i| {
                RoundedRect::new(
                    Rect::new(
                        6.dp() + 9.5.dp() * f64::from(i % 9),
                        10.dp(),
                        8.dp(),
                        28.dp(),
                    ),
                    Corners::all(4.dp()),
                )
                .into()
            }),
            6.dp(),
        )?,
    ])
}

fn glass_pixels(
    painter: &mut Painter,
    shape: &Shape,
    refraction: Dp,
    destination: Destination,
) -> Result<Pixels<f32>, TestError> {
    let glass = lavashell_core::effects::Glass {
        refraction: NonNegativeDp::try_new(refraction)?,
        dispersion: NonNegativeDp::ZERO,
        rim: NonNegative::ZERO,
    };
    let effect = Backdrop::glass(Dp::ZERO, glass)?;
    let mut s = EFFECT_SURFACE.raster()?;
    crate::effect_checks::draw_with(painter, &mut s, |d| {
        stripes(d);
        if destination == Destination::Layer {
            d.commands.push(DrawCommand::Opacity(0.5));
        }
        d.material(
            shape.clone(),
            &Material::new(Color::TRANSPARENT).backdrop(effect),
        );
        if destination == Destination::Layer {
            d.restore();
        }
    })?;
    Ok(crate::effect_checks::linear_pixels(&mut s).map(|[red, ..]| *red))
}

fn changed(
    shape: &Shape,
    plain: &Pixels<f32>,
    glass: &Pixels<f32>,
) -> Result<(usize, usize), TestError> {
    let mut inside = 0;
    let mut outside = 0;
    for ((x, y), &(a, b)) in plain.zipped(glass)?.positioned()? {
        let p = Point::new(Dp::new(f64::from(x) + 0.5), Dp::new(f64::from(y) + 0.5));
        if (a - b).abs() > 0.01 {
            if shape.distance(p) < 0.0 {
                inside += 1;
            } else if shape.distance(p) > 1.0 {
                outside += 1;
            }
        }
    }
    Ok((inside, outside))
}

fn assert_refracts_only_inside(
    shapes: impl IntoIterator<Item = Shape>,
    destination: Destination,
) -> Result<(), TestError> {
    let mut painter = Painter::without_device(&Fonts::new())?;
    for shape in shapes {
        let plain = glass_pixels(&mut painter, &shape, 0.dp(), destination)?;
        let glass = glass_pixels(&mut painter, &shape, 9.dp(), destination)?;
        let (inside, outside) = changed(&shape, &plain, &glass)?;
        assert!(inside > 100, "{shape:?}: {inside} refracted pixels");
        assert_eq!(outside, 0, "{shape:?}");
    }
    Ok(())
}

#[test]
fn glass_refracts_inside_every_shape_and_nowhere_outside() -> Result<(), TestError> {
    assert_refracts_only_inside(shapes()?, Destination::Surface)
}

#[test]
fn glass_keeps_its_lens_in_layers() -> Result<(), TestError> {
    let [rounded, union, contour, ..] = shapes()?;
    assert_refracts_only_inside([rounded, union, contour], Destination::Layer)
}

#[test]
fn glass_that_cannot_read_the_surface_for_its_lens_blurs_and_reports_it() -> Result<(), TestError> {
    let [.., long_contour, many_members] = shapes()?;
    let glass = GlassPrograms::new(&FieldRoom::new(UniformRange::WITHOUT_DEVICE)?)?;
    let resources = PaintResources::new(
        &FieldRoom::new(UniformRange::WITHOUT_DEVICE)?,
        Failures::default(),
    )?;
    for shape in [long_contour, many_members] {
        let mut plain = EFFECT_SURFACE.raster()?;
        background(plain.canvas())?;
        let mut surface = EFFECT_SURFACE.raster()?;
        background(surface.canvas())?;
        let lens = lavashell_core::effects::Glass {
            refraction: constant!(NonNegativeDp::try_new(Dp::new(9.0))),
            dispersion: NonNegativeDp::ZERO,
            rim: NonNegative::ZERO,
        };
        let drawn = glass.backdrop(
            surface.canvas(),
            &resources,
            &shape,
            BlurLevel::new(&Backdrop::glass(4.dp(), lens)?, 0),
            SurfaceAccess::Unreadable,
        );
        assert_eq!(
            drawn.map_err(|failed| failed.failure()),
            Err(DrawFailure::LensWithoutSurface),
            "{shape:?}"
        );
        assert_ne!(
            pixels(&mut plain),
            pixels(&mut surface),
            "{shape:?} still blurs"
        );
    }
    Ok(())
}
