// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_checks::{draw, linear_pixels, stripes};
use crate::test_error::TestError;
use crate::test_surfaces::EFFECT_SURFACE;
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;

pub(crate) fn tangle() -> Result<Shape, TestError> {
    let points = (0..200_u8).map(|i| {
        let angle = f64::from(i) * 2.399;
        Point::new(
            Dp::new(angle.cos()),
            Dp::new(angle.sin() + f64::from(i % 3) * 1e-4),
        )
    });
    Ok(Shape::Contour(Polygon::new(points.collect::<Vec<_>>())?))
}

#[test]
fn a_tangled_contour_draws_its_fill_and_its_glass() -> Result<(), TestError> {
    let mut expected = EFFECT_SURFACE.raster()?;
    draw(&mut expected, stripes)?;
    let shape = tangle()?;
    let fill = DrawCommand::Fill {
        shape: shape.clone(),
        brush: Color::rgb(0x00ff_0000).into(),
    };
    let glass = DrawCommand::Backdrop {
        shape,
        effect: constant!(Backdrop::glass(
            Dp::new(2.0),
            Glass {
                refraction: constant!(NonNegativeDp::try_new(Dp::new(12.0))),
                dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                rim: constant!(NonNegative::try_new(0.14)),
            },
        )),
    };
    for (label, command) in [("fill", fill), ("glass", glass)] {
        let mut drawn = EFFECT_SURFACE.raster()?;
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(EFFECT_SURFACE.size(), Density::ONE));
        stripes(&mut Drawing::new(
            &mut commands,
            EFFECT_SURFACE.bounds(),
            &context,
        ));
        commands.extend([
            DrawCommand::Save,
            DrawCommand::Transform(Affine::scale(16.0, 16.0).translated(EFFECT_SURFACE.center())),
            command,
            DrawCommand::Restore,
        ]);
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        painter.paint_as_leaf(drawn.canvas(), &commands, Density::ONE)?;
        let (drawn, expected) = (linear_pixels(&mut drawn), linear_pixels(&mut expected));
        let radius = |position| EFFECT_SURFACE.pixel_distance_from_center(position);
        let inside = (drawn.positioned()?)
            .filter(|(position, _)| radius(*position) < 14.0)
            .count();
        let mut changed = 0;
        for (x, y) in drawn.positions_differing_in_bits(&expected)? {
            let (pixel, stripe) = (drawn.at(x, y), expected.at(x, y));
            assert!(
                radius((x, y)) <= 17.5,
                "{label} drew {pixel:?} over {stripe:?} at {x}, {y}"
            );
            changed += usize::from(radius((x, y)) < 14.0);
        }
        assert!(
            changed * 2 >= inside,
            "{label} changed {changed} of {inside} pixels inside"
        );
    }
    Ok(())
}
