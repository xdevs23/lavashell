// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Density, Dp, FiniteDp, Fraction, NonNegative, NonNegativeDp, Positive, PositiveDp, ScaleFactor,
};
use crate::encoding::checked_serde;

checked_serde!(
    NonNegative(f64),
    NonNegativeDp(Dp),
    FiniteDp(Dp),
    Fraction(f64),
    Positive(f64),
    PositiveDp(Dp),
    ScaleFactor(f64),
    Density(f64),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestResult;
    use crate::units::DpExt;

    #[test]
    fn a_checked_number_reads_back_as_written() -> TestResult {
        let length = NonNegativeDp::try_new(2.5.dp())?;
        let bytes = postcard::to_allocvec(&length)?;
        assert_eq!(postcard::from_bytes::<NonNegativeDp>(&bytes)?, length);
        Ok(())
    }

    #[test]
    fn a_number_that_fails_its_check_is_not_read() -> TestResult {
        let negative = postcard::to_allocvec(&(-1).dp())?;
        assert!(postcard::from_bytes::<NonNegativeDp>(&negative).is_err());
        let zero = postcard::to_allocvec(&0.0_f64)?;
        assert!(postcard::from_bytes::<Density>(&zero).is_err());
        let above_one = postcard::to_allocvec(&1.5_f64)?;
        assert!(postcard::from_bytes::<Fraction>(&above_one).is_err());
        Ok(())
    }
}
