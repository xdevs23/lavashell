// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Point, Rect, Size};
use lavashell_core::shape::{Corners, PointsError, Polygon, RoundedRect};
use lavashell_core::surface::SurfaceOutline;
use lavashell_core::units::Dp;
use std::rc::Rc;

/// The outline of a surface in surface-local dp. The same outline clips what the surface paints and sets the region
/// in which the surface receives input.
#[derive(Clone)]
pub struct WindowShape {
    outline: Rc<dyn Fn(Size) -> Result<SurfaceOutline, PointsError>>,
    clip_paint: bool,
    accept_input: bool,
}

impl WindowShape {
    /// A shape made of the closed contours returned by `outline` for the size of the surface, with points in dp.
    /// The contours are filled by the even-odd rule, so a contour inside another one cuts a hole. `outline` runs
    /// again when the size of the surface or the reactive state read by it changes. Every contour needs at least
    /// three finite points.
    pub fn new(outline: impl Fn(Size) -> Vec<Vec<Point>> + 'static) -> Self {
        Self::from_outline(move |size| {
            let contours: Vec<Polygon> = (outline(size).into_iter())
                .map(Polygon::new)
                .collect::<Result<_, _>>()?;
            Ok(if contours.is_empty() {
                SurfaceOutline::Empty
            } else {
                SurfaceOutline::Contours(contours)
            })
        })
    }

    /// A shape of one rounded rectangle in surface-local dp, returned by `rect` with its corner radius for the size
    /// of the surface. The rectangle clips painting and the background effects and sets the input region. When
    /// `rect` returns `None`, the shape is empty. `rect` runs again when the reactive state read by it changes.
    pub fn rounded_rect(rect: impl Fn(Size) -> Option<(Rect, Dp)> + 'static) -> Self {
        Self::from_outline(move |size| {
            Ok(match rect(size) {
                Some((rect, radius)) => {
                    SurfaceOutline::Rounded(RoundedRect::new(rect, Corners::all(radius)))
                }
                None => SurfaceOutline::Empty,
            })
        })
    }

    fn from_outline(
        outline: impl Fn(Size) -> Result<SurfaceOutline, PointsError> + 'static,
    ) -> Self {
        Self {
            outline: Rc::new(outline),
            clip_paint: true,
            accept_input: true,
        }
    }

    /// Uses the outline only for the input region. The surface also paints outside the outline.
    #[must_use]
    pub fn input_only(mut self) -> Self {
        self.clip_paint = false;
        self
    }

    /// Uses the outline to clip painting and the background effects. The input region sent to the host is empty.
    #[must_use]
    pub fn input_passthrough(mut self) -> Self {
        self.accept_input = false;
        self
    }

    /// The outline for a surface of `size`.
    ///
    /// # Errors
    ///
    /// Fails when a contour has fewer than three points or a point that is not finite.
    pub fn outline(&self, size: Size) -> Result<SurfaceOutline, PointsError> {
        (self.outline)(size)
    }

    #[must_use]
    pub fn clips_paint(&self) -> bool {
        self.clip_paint
    }

    /// Whether the outline sets the input region. Otherwise the input region is empty.
    #[must_use]
    pub fn accepts_input(&self) -> bool {
        self.accept_input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lavashell_core::units::DpExt;

    #[test]
    fn contours_follow_the_size_of_the_surface() -> Result<(), PointsError> {
        let shape = WindowShape::new(|size| {
            vec![vec![
                Point::default(),
                Point::new(size.width, 0.dp()),
                Point::new(0.dp(), size.height),
            ]]
        });
        let size = Size::new(30.dp(), 20.dp());
        let triangle = Polygon::new([
            Point::default(),
            Point::new(30.dp(), 0.dp()),
            Point::new(0.dp(), 20.dp()),
        ])?;
        assert_eq!(
            shape.outline(size)?,
            SurfaceOutline::Contours(vec![triangle])
        );
        Ok(())
    }

    #[test]
    fn an_invalid_contour_fails_and_no_contours_are_an_empty_outline() {
        let size = Size::new(30.dp(), 20.dp());
        let point = WindowShape::new(|_| vec![vec![Point::default()]]);
        assert_eq!(point.outline(size), Err(PointsError::TooFew));
        let nothing = WindowShape::new(|_| Vec::new());
        assert_eq!(nothing.outline(size), Ok(SurfaceOutline::Empty));
    }

    #[test]
    fn a_missing_rounded_rectangle_is_an_empty_outline() -> Result<(), PointsError> {
        let rect = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
        let shape =
            WindowShape::rounded_rect(move |size| (size.width > 5.dp()).then_some((rect, 2.dp())));
        let rounded = RoundedRect::new(rect, Corners::all(2.dp()));
        let size = |width| Size::new(width, 10.dp());
        assert_eq!(
            shape.outline(size(10.dp()))?,
            SurfaceOutline::Rounded(rounded)
        );
        assert_eq!(shape.outline(size(1.dp()))?, SurfaceOutline::Empty);
        Ok(())
    }

    #[test]
    fn passthrough_clips_without_input_and_input_only_takes_input_without_clipping() {
        let shape = WindowShape::new(|_| Vec::new());
        let passthrough = shape.clone().input_passthrough();
        assert!(passthrough.clips_paint() && !passthrough.accepts_input());
        let input_only = shape.input_only();
        assert!(!input_only.clips_paint() && input_only.accepts_input());
    }
}
