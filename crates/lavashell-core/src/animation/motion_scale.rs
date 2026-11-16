// SPDX-License-Identifier: GPL-3.0-only
use super::VectorSpace;

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

    pub(super) fn reduced<V: VectorSpace>(self, vector: &V) -> V {
        vector.scale(self.factor)
    }

    pub(super) fn restored<V: VectorSpace>(self, vector: &V) -> V {
        vector.saturating_scale(1.0 / self.factor)
    }

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
