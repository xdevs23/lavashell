// SPDX-License-Identifier: GPL-3.0-only
use super::VectorSpace;

/// The power of two that brings the largest component of the vectors of a motion to a magnitude between one and two.
/// A motion is evaluated with its vectors multiplied by this factor and its results divided by it. Multiplying by a
/// power of two changes only the exponent of a number, so the motion computes the same digits at every magnitude,
/// and no sum or difference of its finite vectors overflows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MotionScale {
    factor: f64,
}

impl MotionScale {
    pub(super) fn of<V: VectorSpace>(vectors: &[&V]) -> Self {
        let largest = vectors.iter().fold(0.0_f64, |a, v| a.max(v.max_abs()));
        Self {
            factor: unit_factor(largest),
        }
    }

    /// `vector`, given in the units of the motion, in this scale.
    pub(super) fn reduced<V: VectorSpace>(self, vector: &V) -> V {
        vector.scale(self.factor)
    }

    /// `vector`, given in this scale, in the units of the motion. A component beyond the range of f64 saturates at
    /// `±f64::MAX`.
    pub(super) fn restored<V: VectorSpace>(self, vector: &V) -> V {
        vector.saturating_scale(1.0 / self.factor)
    }

    /// The position and the velocity that `motion` computes from `vectors`, evaluated in the scale of `vectors` and
    /// returned in their units. `motion` must scale its results by the factor that scales all of its vectors.
    pub(super) fn sampled<V: VectorSpace, const N: usize>(
        vectors: [&V; N],
        motion: impl FnOnce([&V; N]) -> (V, V),
    ) -> (V, V) {
        let scale = Self::of(&vectors);
        let reduced = vectors.map(|vector| scale.reduced(vector));
        let (position, velocity) = motion(reduced.each_ref());
        (scale.restored(&position), scale.restored(&velocity))
    }
}

/// `2^-e` for `largest = m · 2^e` with `1 ≤ m < 2`, or one when `largest` is zero or subnormal and cannot overflow.
/// Clearing the significand of `largest` leaves `2^e`, and the inverse of a power of two is exact.
fn unit_factor(largest: f64) -> f64 {
    if largest.is_normal() {
        1.0 / f64::from_bits(largest.to_bits() & f64::INFINITY.to_bits())
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Vector;
    use crate::float_asserts::assert_f64_eq;

    #[test]
    fn the_largest_component_is_brought_between_one_and_two() {
        for largest in [3.0, 0.75, f64::MAX, f64::MIN_POSITIVE, 1.0] {
            let scale = MotionScale::of(&[&Vector([largest]), &Vector([-largest / 2.0])]);
            let reduced = scale.reduced(&Vector([largest])).0[0];
            assert!((1.0..2.0).contains(&reduced), "{largest} became {reduced}");
            assert_eq!(scale.restored(&Vector([reduced])), Vector([largest]));
        }
        let still = MotionScale::of(&[&Vector([0.0])]);
        assert_eq!(still.reduced(&Vector([0.0])), Vector([0.0]));
    }

    #[test]
    fn a_restored_component_beyond_the_range_saturates() {
        let scale = MotionScale::of(&[&Vector([f64::MAX])]);
        let restored = scale.restored(&Vector([4.0, -4.0, 1.0]));
        assert_f64_eq(restored.0[0], f64::MAX);
        assert_f64_eq(restored.0[1], -f64::MAX);
        assert_f64_eq(restored.0[2], 2.0_f64.powi(1023));
    }
}
