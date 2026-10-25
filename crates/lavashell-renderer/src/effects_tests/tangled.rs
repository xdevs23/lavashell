// SPDX-License-Identifier: GPL-3.0-only
use super::{draw, sample, stripes, surface};
use crate::test_error::TestError;
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;

/// A polygon with 200 corners on the unit circle whose edges cross each other about 15000 times, some of them close
/// to the end of another edge.
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

/// Draws the tangled contour, scaled to a radius of 16 pixels, as a fill and as glass over stripes. Pixels more than
/// 17.5 pixels from the center are unchanged. The edges lie a fraction of a pixel apart and the contour fills about
/// half of its circle, so at least half of the pixels within 14 pixels of the center change.
#[test]
fn a_tangled_contour_draws_its_fill_and_its_glass() -> Result<(), TestError> {
    let mut expected = surface()?;
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
        let mut drawn = surface()?;
        let mut commands = Vec::new();
        let context = DrawContext::new(Viewport::new(Size::new(96.dp(), 48.dp()), Density::ONE));
        stripes(&mut Drawing::new(
            &mut commands,
            Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
            &context,
        ));
        commands.extend([
            DrawCommand::Save,
            DrawCommand::Transform(
                Affine::scale(16.0, 16.0).translated(Point::new(48.dp(), 24.dp())),
            ),
            command,
            DrawCommand::Restore,
        ]);
        let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        painter.paint_as_leaf(drawn.canvas(), &commands, Density::ONE)?;
        let (mut inside, mut changed) = (0, 0);
        for (x, y) in (0..48_i16).flat_map(|y| (0..96_i16).map(move |x| (x, y))) {
            let radius = (f64::from(x) + 0.5 - 48.0).hypot(f64::from(y) + 0.5 - 24.0);
            let (pixel, stripe) = (
                sample(&mut drawn, i32::from(x), i32::from(y)),
                sample(&mut expected, i32::from(x), i32::from(y)),
            );
            let same = pixel.map(f32::to_bits) == stripe.map(f32::to_bits);
            if radius > 17.5 {
                assert!(same, "{label} drew {pixel:?} over {stripe:?} at {x}, {y}");
            } else if radius < 14.0 {
                inside += 1;
                changed += usize::from(!same);
            }
        }
        assert!(
            changed * 2 >= inside,
            "{label} changed {changed} of {inside} pixels inside"
        );
    }
    Ok(())
}
