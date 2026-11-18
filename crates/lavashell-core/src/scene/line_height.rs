// SPDX-License-Identifier: GPL-3.0-only
use super::FontSize;
use crate::units::{Dp, NumberError, Positive};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineHeight<S: FontSize>(S);

impl<S: FontSize> LineHeight<S> {
    pub fn try_new(length: S) -> Result<Self, NumberError> {
        Positive::try_new(length.number()).map(|_same_number_as_length| Self(length))
    }

    #[must_use]
    pub fn get(self) -> S {
        self.0
    }

    pub fn multiple_of(self, font_size: S) -> Result<Positive, NumberError> {
        Positive::try_new(self.0.number() / font_size.number())
    }

    pub(super) fn rescale(self, font_size: S, new_font_size: Dp) -> Option<LineHeight<Dp>> {
        (self.multiple_of(font_size))
            .and_then(|multiple| LineHeight::try_new(new_font_size * multiple.get()))
            .ok()
    }
}

#[cfg(feature = "serde")]
impl<S: FontSize + serde::Serialize> serde::Serialize for LineHeight<S> {
    fn serialize<Z: serde::Serializer>(&self, serializer: Z) -> Result<Z::Ok, Z::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, S: FontSize + serde::Deserialize<'de>> serde::Deserialize<'de> for LineHeight<S> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_new(S::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;
    use crate::test_error::TestResult;
    use crate::units::DpExt;

    #[test]
    fn a_line_height_without_a_positive_finite_length_is_not_read() -> TestResult {
        for length in [f64::NAN, 0.0, -24.0] {
            let bytes = postcard::to_allocvec(&length.dp())?;
            assert!(
                postcard::from_bytes::<LineHeight<Dp>>(&bytes).is_err(),
                "{length}"
            );
        }
        Ok(())
    }
}
