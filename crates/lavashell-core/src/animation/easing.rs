// SPDX-License-Identifier: GPL-3.0-only
/// An easing curve, which maps the progress of an animation from 0 to 1 to the eased progress. `sample` returns the
/// value of the curve and its derivative.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Easing {
    Linear,
    /// The quartic ease-out curve `1 - (1 - x)^4`. `sample` returns its derivative exactly, also at the endpoints.
    QuartOut,
    Smooth,
    /// The minimum-jerk curve, whose velocity and acceleration are zero at both ends.
    Smoother,
    /// A cubic Bezier from (0, 0) to (1, 1), made by `Easing::cubic_bezier` or `cubic_bezier!`.
    CubicBezier(CubicBezier),
}

/// The two inner control points of a cubic Bezier easing curve, checked by `Easing::cubic_bezier`. The outer control
/// points are (0, 0) and (1, 1).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "Controls")
)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(rename = "CubicBezier")]
struct Controls {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

#[cfg(feature = "serde")]
impl TryFrom<Controls> for CubicBezier {
    type Error = EasingError;

    fn try_from(Controls { x1, y1, x2, y2 }: Controls) -> Result<Self, EasingError> {
        Self::checked(x1, y1, x2, y2)
    }
}

/// The reason why the control points given to `Easing::cubic_bezier` do not form an easing curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EasingError {
    #[error("the controls of a cubic Bezier must be finite")]
    NotFinite,
    /// An x control outside 0..=1 can make the curve run backwards in x. Such a curve is not a function of x.
    #[error("the x controls of a cubic Bezier must be within 0 and 1")]
    ControlOutOfRange,
    /// Where the curve is vertical, its slope and the velocity of a motion along it are infinite.
    #[error("a cubic Bezier must not have a vertical tangent")]
    VerticalTangent,
}

impl Easing {
    pub const EASE: Self = crate::cubic_bezier!(0.25, 0.1, 0.25, 1.0);

    /// # Errors
    ///
    /// Fails when a control is not finite, an x control is not within 0 and 1, or the curve has a vertical
    /// tangent.
    pub const fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, EasingError> {
        match CubicBezier::checked(x1, y1, x2, y2) {
            Ok(curve) => Ok(Self::CubicBezier(curve)),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub fn sample(self, x: f64) -> (f64, f64) {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Linear => (x, 1.0),
            Self::QuartOut => (1.0 - (1.0 - x).powi(4), 4.0 * (1.0 - x).powi(3)),
            Self::Smooth => (x * x * (3.0 - 2.0 * x), 6.0 * x * (1.0 - x)),
            Self::Smoother => (
                x * x * x * (10.0 + x * (-15.0 + 6.0 * x)),
                30.0 * x * x * (1.0 - x).powi(2),
            ),
            Self::CubicBezier(curve) => {
                let t = parameter_at(curve.x1, curve.x2, x);
                (unit_cubic(t, curve.y1, curve.y2), curve.slope(t))
            }
        }
    }
}

impl CubicBezier {
    const fn checked(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, EasingError> {
        if !(x1.is_finite() && y1.is_finite() && x2.is_finite() && y2.is_finite()) {
            return Err(EasingError::NotFinite);
        }
        if x1 < 0.0 || x1 > 1.0 || x2 < 0.0 || x2 > 1.0 {
            return Err(EasingError::ControlOutOfRange);
        }
        let curve = Self { x1, y1, x2, y2 };
        if curve.vertical() {
            return Err(EasingError::VerticalTangent);
        }
        Ok(curve)
    }

    /// Whether the curve has a vertical tangent: a parameter at which the derivative of x is zero and the
    /// derivative of y is not. With the x controls within 0 and 1, the derivative of x is zero only at the start
    /// when `x1` is 0, at the end when `x2` is 1, and halfway when `x1` is 1 and `x2` is 0. Where the derivatives of
    /// x are zero up to some order, the derivatives of y must be zero up to the same order. The checks at the end use
    /// the distances of the controls from (1, 1), because those distances are exact for controls near 1.
    const fn vertical(self) -> bool {
        let start = self.x1 == 0.0 && (self.y1 != 0.0 || (self.x2 == 0.0 && self.y2 != 0.0));
        let [x1_from_end, y1_from_end] = [1.0 - self.x1, 1.0 - self.y1];
        let [x2_from_end, y2_from_end] = [1.0 - self.x2, 1.0 - self.y2];
        let end = x2_from_end == 0.0
            && (y2_from_end != 0.0 || (x1_from_end == 0.0 && y1_from_end != 0.0));
        let halfway =
            x1_from_end == 0.0 && self.x2 == 0.0 && (y1_from_end != 0.0 || self.y2 != 0.0);
        start || end || halfway
    }

    /// The slope dy/dx at the parameter `t`: the ratio of the derivatives of y and x by `t` of the lowest order at
    /// which the derivative of x is not zero. That ratio is the limit of the slope at the parameters near `t`. A
    /// curve that is not constant in x has such an order at every parameter.
    fn slope(self, t: f64) -> f64 {
        let x = unit_cubic_derivatives(t, self.x1, self.x2);
        let y = unit_cubic_derivatives(t, self.y1, self.y2);
        let (dx, dy) = (x.into_iter().zip(y).rev()).fold((x[2], y[2]), |lower, (dx, dy)| {
            if dx == 0.0 { lower } else { (dx, dy) }
        });
        dy / dx
    }
}

/// An `Easing::CubicBezier` from constant controls, checked when the program is compiled.
#[macro_export]
macro_rules! cubic_bezier {
    ($x1:expr, $y1:expr, $x2:expr, $y2:expr $(,)?) => {
        $crate::constant!($crate::animation::Easing::cubic_bezier($x1, $y1, $x2, $y2))
    };
}

/// A cubic Bezier from 0 to 1 with the controls `a` and `b`, at the parameter `t`.
fn unit_cubic(t: f64, a: f64, b: f64) -> f64 {
    3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t
}

/// The derivatives of `unit_cubic` by `t` of the first, second and third order.
fn unit_cubic_derivatives(t: f64, a: f64, b: f64) -> [f64; 3] {
    let cubic = 3.0 * a - 3.0 * b + 1.0;
    let square = 3.0 * b - 6.0 * a;
    [
        3.0 * a + t * (2.0 * square + t * 3.0 * cubic),
        2.0 * square + t * 6.0 * cubic,
        6.0 * cubic,
    ]
}

/// The first parameter at which the x curve with the controls `x1` and `x2` is at least `x`. The controls are
/// within 0 and 1, so the curve never runs backwards. The bisection ends when no float lies between the ends of
/// its bracket.
fn parameter_at(x1: f64, x2: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0, 1.0);
    loop {
        let midpoint = f64::midpoint(low, high);
        if midpoint <= low || midpoint >= high {
            return high;
        }
        if unit_cubic(midpoint, x1, x2) < x {
            low = midpoint;
        } else {
            high = midpoint;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float_asserts::assert_f64_eq;

    #[test]
    fn controls_must_be_finite_and_their_x_within_zero_and_one() {
        assert_eq!(
            Easing::cubic_bezier(f64::NAN, 0.0, 1.0, 1.0),
            Err(EasingError::NotFinite)
        );
        assert_eq!(
            Easing::cubic_bezier(0.5, f64::INFINITY, 0.5, 1.0),
            Err(EasingError::NotFinite)
        );
        assert_eq!(
            Easing::cubic_bezier(0.5, 0.0, 1.5, 1.0),
            Err(EasingError::ControlOutOfRange)
        );
        assert!(Easing::cubic_bezier(0.5, -3.0, 0.5, 4.0).is_ok());
    }

    #[test]
    fn the_curve_starts_at_zero_and_ends_at_one_exactly() {
        for curve in [Easing::EASE, cubic_bezier!(0.16, 1.0, 0.3, 1.0)] {
            assert_f64_eq(curve.sample(0.0).0, 0.0);
            assert_f64_eq(curve.sample(1.0).0, 1.0);
        }
    }

    #[test]
    fn the_inverted_x_curve_reaches_x_at_the_parameter_and_not_one_float_before() {
        for (x1, x2) in [(0.0, 0.0), (0.25, 0.25), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            for step in 1..1000 {
                let x = f64::from(step) / 1000.0;
                let t = parameter_at(x1, x2, x);
                assert!(
                    unit_cubic(t.next_down(), x1, x2) < x && unit_cubic(t, x1, x2) >= x,
                    "{x1} {x2} {x}"
                );
            }
        }
    }

    #[test]
    fn a_tangent_without_x_and_y_takes_the_slope_around_it() {
        for curve in [
            cubic_bezier!(0.0, 0.0, 1.0, 1.0),
            cubic_bezier!(1.0, 1.0, 0.0, 0.0),
        ] {
            for x in [0.0, 0.5, 1.0] {
                assert_f64_eq(curve.sample(x).1, 1.0);
            }
        }
    }

    #[test]
    fn a_vertical_tangent_makes_no_easing_curve() {
        for (x1, y1, x2, y2) in [
            (0.0, 1.0, 1.0, 1.0),
            (0.0, 0.0, 0.0, 1.0),
            (0.5, 0.0, 1.0, 0.5),
            (1.0, 0.5, 0.0, 0.5),
        ] {
            assert_eq!(
                Easing::cubic_bezier(x1, y1, x2, y2),
                Err(EasingError::VerticalTangent)
            );
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn controls_that_fail_the_checks_are_not_read() -> crate::test_error::TestResult {
        let controls = (2.0_f64, 0.0_f64, 0.5_f64, 1.0_f64);
        let bytes = postcard::to_allocvec(&controls)?;
        assert!(postcard::from_bytes::<CubicBezier>(&bytes).is_err());
        let bytes = postcard::to_allocvec(&Easing::EASE)?;
        assert_eq!(postcard::from_bytes::<Easing>(&bytes)?, Easing::EASE);
        Ok(())
    }
}
