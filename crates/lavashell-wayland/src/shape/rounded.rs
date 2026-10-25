// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, Error, Outline, Path, PathBuilder, Rc, Size, WindowShape, scalar, skia};
use lavashell_core::geometry::Rect;
use lavashell_renderer::skia::RRect;

pub(super) type RoundedRect = dyn Fn(Size) -> Option<(Rect, Dp)>;

impl WindowShape {
    /// A shape of one rounded rectangle in surface-local dp, returned by `rect` with its corner radius for the size
    /// of the surface. The rectangle clips painting and the background effects and sets the input region. When
    /// `rect` returns `None`, the shape is empty. `rect` runs again when the reactive state read by it changes.
    pub fn rounded_rect(rect: impl Fn(Size) -> Option<(Rect, Dp)> + 'static) -> Self {
        Self::from_outline(Outline::RoundedRect(Rc::new(rect)))
    }
}

/// The path in dp around `rect` with corners of `radius`. Skia reduces a radius that does not fit the shorter side
/// of `rect`.
pub(super) fn path(rect: Option<(Rect, Dp)>) -> Result<Path, Error> {
    let Some((rect, radius)) = rect else {
        return Ok(Path::new());
    };
    if ![rect.x, rect.y, rect.width, rect.height, radius]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(Error::RoundedShape(rect, radius));
    }
    if rect.width <= Dp::ZERO || rect.height <= Dp::ZERO {
        return Ok(Path::new());
    }
    let to_scalar = |length: Dp| scalar(length.get());
    let bounds = skia::Rect::from_xywh(
        to_scalar(rect.x)?,
        to_scalar(rect.y)?,
        to_scalar(rect.width)?,
        to_scalar(rect.height)?,
    );
    let radius = to_scalar(radius)?;
    let corners = RRect::new_rect_xy(bounds, radius, radius);
    Ok(PathBuilder::new().add_rrect(corners, None, None).detach())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::tests::{density_for, takes_input};
    use crate::test_error::TestError;
    use lavashell_core::units::Density;
    use lavashell_core::units::DpExt;

    #[test]
    fn fractional_rounded_regions_exclude_transparent_corners_for_paint_and_input()
    -> Result<(), TestError> {
        for phase in [0.0, 0.25, 0.75] {
            let shape = WindowShape::rounded_rect(move |_| {
                Some((
                    Rect::new(Dp::new(phase), Dp::new(phase), 320.dp(), 124.dp()),
                    14.dp(),
                ))
            });
            for scale in [1.0, 1.5, 2.0] {
                let centre = |cell: i16| scalar((f64::from(cell) + 0.5) * scale);
                let geometry = shape.compile(Size::new(321.dp(), 125.dp()), density_for(scale)?)?;
                for (x, y, expected) in [
                    (1_i16, 1_i16, false),
                    (160, 62, true),
                    (318, 1, false),
                    (318, 122, false),
                ] {
                    let native = takes_input(&geometry, i32::from(x), i32::from(y));
                    let painted = geometry.path.contains((centre(x)?, centre(y)?));
                    assert_eq!(native, expected, "phase {phase} cell {x},{y}");
                    assert_eq!(painted, native);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn missing_and_degenerate_rectangles_are_empty_but_nonfinite_shapes_fail()
    -> Result<(), TestError> {
        assert!(
            WindowShape::rounded_rect(|_| None)
                .compile(Size::new(1.dp(), 1.dp()), Density::ONE)?
                .input_cells
                .is_empty()
        );
        assert!(
            WindowShape::rounded_rect(|_| Some((Rect::default(), 14.dp())))
                .compile(Size::new(1.dp(), 1.dp()), Density::ONE)?
                .input_cells
                .is_empty()
        );
        assert!(
            WindowShape::rounded_rect(|_| Some((
                Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()),
                Dp::new(f64::NAN)
            )))
            .compile(Size::new(10.dp(), 10.dp()), Density::ONE)
            .is_err()
        );
        Ok(())
    }
}
