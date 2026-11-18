// SPDX-License-Identifier: GPL-3.0-only

use crate::failure::DrawFailure;
use num_traits::AsPrimitive;

pub fn scalar(value: f64) -> Result<f32, DrawFailure> {
    let narrowed: f32 = value.as_();
    if value.is_finite() && narrowed.is_infinite() {
        Err(DrawFailure::NumberRange)
    } else {
        Ok(narrowed)
    }
}

pub fn scalars<const N: usize>(values: [f64; N]) -> Result<[f32; N], DrawFailure> {
    let mut narrowed = [0.0; N];
    for (slot, value) in narrowed.iter_mut().zip(values) {
        *slot = scalar(value)?;
    }
    Ok(narrowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finite_number_beyond_the_range_of_f32_fails_and_every_other_number_narrows_to_the_nearest()
    {
        let bits = |value: f64| scalar(value).map(f32::to_bits);
        assert_eq!(bits(0.1), Ok(0.1_f32.to_bits()));
        assert_eq!(bits(f64::from(f32::MAX)), Ok(f32::MAX.to_bits()));
        assert_eq!(bits(f64::NEG_INFINITY), Ok(f32::NEG_INFINITY.to_bits()));
        assert!(scalar(f64::NAN).is_ok_and(f32::is_nan));
        assert_eq!(bits(f64::MAX), Err(DrawFailure::NumberRange));
        assert_eq!(bits(-f64::MAX), Err(DrawFailure::NumberRange));
        assert_eq!(
            scalars([1.5, f64::MAX]).map(|values| values.map(f32::to_bits)),
            Err(DrawFailure::NumberRange)
        );
    }
}
