// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Point, Rect, Size};
use lavashell_core::reactive::{Phase, Scope};
use lavashell_core::shape::{Corners, PointsError, Polygon, RoundedRect};
use lavashell_core::surface::{SurfaceChange, SurfaceOutline};
use lavashell_core::units::{Density, Dp};
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

/// The shape of a surface: a window shape with its input cells, or the whole surface without a window shape. The input
/// cells are computed again when the size or the density of the surface or the reactive state read by the window shape
/// changes.
#[derive(Default)]
pub struct SurfaceShape {
    shape: Option<WindowShape>,
    key: Option<(Size, Density)>,
    cells: Vec<Rect>,
    scope: Scope,
}

impl SurfaceShape {
    #[must_use]
    pub fn new(shape: Option<WindowShape>) -> Self {
        Self {
            shape,
            ..Self::default()
        }
    }

    /// Computes the shape for a surface of `size` at `density`, unless it belongs to that size and density already and
    /// the reactive state read by the window shape is unchanged. `input_cells` returns the input cells in dp for the
    /// outline of the window shape. Returns the changes of the surface state that follow, or no change when nothing
    /// was computed. Without a window shape, the blur region follows the size of the surface.
    ///
    /// # Errors
    ///
    /// Fails when a contour of the window shape is not a valid polygon, or when `input_cells` fails.
    pub fn update<E: From<PointsError>>(
        &mut self,
        size: Size,
        density: Density,
        input_cells: impl FnOnce(&SurfaceOutline) -> Result<Vec<Rect>, E>,
    ) -> Result<Vec<SurfaceChange>, E> {
        if self.key == Some((size, density)) && !self.needs_frame() {
            return Ok(Vec::new());
        }
        let Some(shape) = &self.shape else {
            self.key = Some((size, density));
            return Ok(vec![SurfaceChange::BlurRegion(Some(vec![
                Rect::from_size(size),
            ]))]);
        };
        let outline = self.scope.track(Phase::Layout, || shape.outline(size))?;
        self.cells = input_cells(&outline)?;
        self.key = Some((size, density));
        let input = if shape.accepts_input() {
            self.cells.clone()
        } else {
            Vec::new()
        };
        Ok(vec![
            SurfaceChange::InputRegion(Some(input)),
            SurfaceChange::BlurRegion(Some(self.cells.clone())),
        ])
    }

    #[must_use]
    pub fn needs_frame(&self) -> bool {
        self.scope.dirty().any()
    }

    /// Whether the surface receives input anywhere.
    #[must_use]
    pub fn accepts_input(&self) -> bool {
        (self.shape.as_ref()).is_none_or(|shape| shape.accepts_input() && !self.cells.is_empty())
    }

    #[must_use]
    pub fn clips_paint(&self) -> bool {
        self.shape.as_ref().is_some_and(WindowShape::clips_paint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lavashell_core::units::DpExt;

    #[test]
    fn a_shape_receives_input_in_its_cells_unless_it_passes_input_through()
    -> Result<(), PointsError> {
        let square = Rect::new(0.dp(), 0.dp(), 2.dp(), 2.dp());
        let shape = WindowShape::rounded_rect(move |_| Some((square, 0.dp())));
        let size = Size::new(4.dp(), 4.dp());
        let cells = move |_: &SurfaceOutline| Ok::<_, PointsError>(vec![square]);
        let mut receiving = SurfaceShape::new(Some(shape.clone()));
        assert_eq!(
            receiving.update(size, Density::ONE, cells)?,
            [
                SurfaceChange::InputRegion(Some(vec![square])),
                SurfaceChange::BlurRegion(Some(vec![square])),
            ]
        );
        assert!(receiving.update(size, Density::ONE, cells)?.is_empty());
        assert!(receiving.accepts_input());
        let mut passthrough = SurfaceShape::new(Some(shape.input_passthrough()));
        assert_eq!(
            passthrough.update(size, Density::ONE, cells)?,
            [
                SurfaceChange::InputRegion(Some(Vec::new())),
                SurfaceChange::BlurRegion(Some(vec![square])),
            ]
        );
        assert!(!passthrough.accepts_input());
        Ok(())
    }

    #[test]
    fn without_a_window_shape_only_the_blur_region_follows_the_size() -> Result<(), PointsError> {
        let mut whole = SurfaceShape::default();
        let size = Size::new(4.dp(), 4.dp());
        let cells = |_: &SurfaceOutline| Ok::<_, PointsError>(Vec::new());
        assert_eq!(
            whole.update(size, Density::ONE, cells)?,
            [SurfaceChange::BlurRegion(Some(vec![Rect::from_size(size)]))]
        );
        assert!(whole.update(size, Density::ONE, cells)?.is_empty());
        assert!(whole.accepts_input() && !whole.clips_paint());
        Ok(())
    }

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
