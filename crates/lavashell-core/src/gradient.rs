// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    color::{Color, ColorInterpolation},
    geometry::Point,
    numbers::count_f64,
    units::{Dp, NumberError},
};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ColorStop {
    pub position: f64,
    pub color: Color,
}

impl ColorStop {
    #[must_use]
    pub const fn new(position: f64, color: Color) -> Self {
        Self { position, color }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TileMode {
    #[default]
    Clamp,
    Repeat,
    Mirror,
    Decal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GradientKind {
    Linear {
        from: Point,
        to: Point,
    },
    Radial {
        center: Point,
        radius: Dp,
    },
    Sweep {
        center: Point,
        start: f64,
        end: f64,
    },
    Conical {
        start: Point,
        start_radius: Dp,
        end: Point,
        end_radius: Dp,
    },
}

impl GradientKind {
    /// The numbers that place this kind, in the order of its fields: x and y of each point, each
    /// radius in dp, and the start and end angles of a sweep.
    pub(crate) fn coordinates(&self) -> Vec<f64> {
        let mut numbers = Vec::new();
        let mut kind = *self;
        kind.each_number_mut(|number| numbers.push(*number));
        numbers
    }

    /// This kind with its numbers replaced by `numbers`, given in the order of `coordinates`, or None when
    /// `numbers` has another count than the coordinates. A radius below zero becomes zero.
    pub(crate) fn with_coordinates(&self, numbers: impl IntoIterator<Item = f64>) -> Option<Self> {
        let mut numbers = numbers.into_iter();
        let mut complete = true;
        let mut kind = *self;
        kind.each_number_mut(|number| match numbers.next() {
            Some(replacement) => *number = replacement,
            None => complete = false,
        });
        (complete && numbers.next().is_none()).then_some(kind)
    }

    fn each_number_mut(&mut self, mut visit: impl FnMut(&mut f64)) {
        let visit = &mut visit;
        match self {
            Self::Linear { from, to } => {
                visit_point(visit, from);
                visit_point(visit, to);
            }
            Self::Radial { center, radius } => {
                visit_point(visit, center);
                visit_radius(visit, radius);
            }
            Self::Sweep { center, start, end } => {
                visit_point(visit, center);
                visit(start);
                visit(end);
            }
            Self::Conical {
                start,
                start_radius,
                end,
                end_radius,
            } => {
                visit_point(visit, start);
                visit_radius(visit, start_radius);
                visit_point(visit, end);
                visit_radius(visit, end_radius);
            }
        }
    }
}

fn visit_point(visit: &mut impl FnMut(&mut f64), point: &mut Point) {
    let (mut x, mut y) = (point.x.get(), point.y.get());
    visit(&mut x);
    visit(&mut y);
    *point = Point::new(Dp::new(x), Dp::new(y));
}

fn visit_radius(visit: &mut impl FnMut(&mut f64), radius: &mut Dp) {
    let mut number = radius.get();
    visit(&mut number);
    *radius = Dp::new(number).max(Dp::ZERO);
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Gradient {
    pub kind: GradientKind,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub stops: Vec<ColorStop>,
    pub tile: TileMode,
    pub interpolation: ColorInterpolation,
}

impl Gradient {
    pub fn new(kind: GradientKind, stops: impl Into<Vec<ColorStop>>) -> Self {
        Self {
            kind,
            stops: stops.into(),
            tile: TileMode::Clamp,
            interpolation: ColorInterpolation::default(),
        }
    }

    /// A gradient from `from` to `to` through `colors` at equal distances.
    ///
    /// # Errors
    ///
    /// Fails when the number of colors is too large for an f64 to represent exactly.
    pub fn linear(from: Point, to: Point, colors: &[Color]) -> Result<Self, NumberError> {
        let gaps = count_f64(colors.len().saturating_sub(1).max(1))?;
        let stops = (colors.iter().enumerate())
            .map(|(index, color)| Ok(ColorStop::new(count_f64(index)? / gaps, *color)))
            .collect::<Result<Vec<_>, NumberError>>()?;
        Ok(Self::new(GradientKind::Linear { from, to }, stops))
    }

    #[must_use]
    pub fn interpolation(mut self, interpolation: ColorInterpolation) -> Self {
        self.interpolation = interpolation;
        self
    }

    #[must_use]
    pub fn tile(mut self, tile: TileMode) -> Self {
        self.tile = tile;
        self
    }

    #[must_use]
    pub fn translated(mut self, offset: Point) -> Self {
        let shift = |p: Point| p.offset(offset.x, offset.y);
        self.kind = match self.kind {
            GradientKind::Linear { from, to } => GradientKind::Linear {
                from: shift(from),
                to: shift(to),
            },
            GradientKind::Radial { center, radius } => GradientKind::Radial {
                center: shift(center),
                radius,
            },
            GradientKind::Sweep { center, start, end } => GradientKind::Sweep {
                center: shift(center),
                start,
                end,
            },
            GradientKind::Conical {
                start,
                start_radius,
                end,
                end_radius,
            } => GradientKind::Conical {
                start: shift(start),
                start_radius,
                end: shift(end),
                end_radius,
            },
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_of_another_count_place_no_gradient() {
        let kind = GradientKind::Radial {
            center: Point::new(Dp::ZERO, Dp::ZERO),
            radius: Dp::new(4.0),
        };
        assert_eq!(kind.with_coordinates([1.0, 2.0]), None);
        assert_eq!(kind.with_coordinates([1.0, 2.0, 3.0, 4.0]), None);
        assert_eq!(
            kind.with_coordinates([1.0, 2.0, -3.0]),
            Some(GradientKind::Radial {
                center: Point::new(Dp::new(1.0), Dp::new(2.0)),
                radius: Dp::ZERO,
            })
        );
    }
}
