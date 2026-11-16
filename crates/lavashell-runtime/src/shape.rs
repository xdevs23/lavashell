// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::{Point, Rect, Size};
use lavashell_core::reactive::{Phase, Scope};
use lavashell_core::shape::{Corners, PointsError, Polygon, RoundedRect};
use lavashell_core::surface::{SurfaceChange, SurfaceOutline};
use lavashell_core::units::{Density, Dp};
use std::rc::Rc;

#[derive(Clone)]
pub struct WindowShape {
    outline: Rc<dyn Fn(Size) -> Result<SurfaceOutline, PointsError>>,
    clip_paint: bool,
    accept_input: bool,
}

impl WindowShape {
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

    #[must_use]
    pub fn input_only(mut self) -> Self {
        self.clip_paint = false;
        self
    }

    #[must_use]
    pub fn input_passthrough(mut self) -> Self {
        self.accept_input = false;
        self
    }

    pub fn outline(&self, size: Size) -> Result<SurfaceOutline, PointsError> {
        (self.outline)(size)
    }

    #[must_use]
    pub fn clips_paint(&self) -> bool {
        self.clip_paint
    }

    #[must_use]
    pub fn accepts_input(&self) -> bool {
        self.accept_input
    }
}

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
        self.scope.is_dirty()
    }

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
mod tests;
