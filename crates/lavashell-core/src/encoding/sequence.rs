// SPDX-License-Identifier: GPL-3.0-only
//! Reads a sequence by appending each element until the input ends.

use serde::de::{self, Deserialize, Deserializer, SeqAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;

/// Reads a sequence by appending each element until the input ends.
///
/// # Errors
///
/// Fails when the input is not a sequence or an element fails to be read.
pub fn sequence<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    deserializer.deserialize_seq(Sequence(PhantomData))
}

/// `sequence` for a sequence that may be missing.
pub(crate) fn optional_sequence<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error> {
    deserializer.deserialize_option(OptionalSequence(PhantomData))
}

struct Sequence<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> Visitor<'de> for Sequence<T> {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut elements: A) -> Result<Vec<T>, A::Error> {
        let mut sequence = Vec::new();
        while let Some(element) = elements.next_element()? {
            sequence.push(element);
        }
        Ok(sequence)
    }
}

struct OptionalSequence<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> Visitor<'de> for OptionalSequence<T> {
    type Value = Option<Vec<T>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence or nothing")
    }

    fn visit_none<E: de::Error>(self) -> Result<Option<Vec<T>>, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Option<Vec<T>>, D::Error> {
        sequence(deserializer).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestResult;

    fn read_numbers(bytes: &[u8]) -> Result<Vec<u64>, postcard::Error> {
        sequence(&mut postcard::Deserializer::from_bytes(bytes))
    }

    fn read_optional(bytes: &[u8]) -> Result<Option<Vec<u64>>, postcard::Error> {
        optional_sequence(&mut postcard::Deserializer::from_bytes(bytes))
    }

    #[test]
    fn a_sequence_reads_every_element_until_the_input_ends() -> TestResult {
        assert_eq!(read_numbers(&[3, 1, 2, 3])?, [1, 2, 3]);
        Ok(())
    }

    #[test]
    fn a_sequence_that_claims_more_elements_than_the_input_contains_fails_at_the_end() {
        let claims_u32_max_elements = [0xff, 0xff, 0xff, 0xff, 0x0f, 1, 2, 3];
        assert_eq!(
            read_numbers(&claims_u32_max_elements),
            Err(postcard::Error::DeserializeUnexpectedEnd)
        );
    }

    #[test]
    fn an_optional_sequence_reads_present_and_missing_values() -> TestResult {
        let present = postcard::to_allocvec(&Some(vec![1_u64, 2, 3]))?;
        assert_eq!(read_optional(&present)?, Some(vec![1, 2, 3]));
        let missing = postcard::to_allocvec(&None::<Vec<u64>>)?;
        assert_eq!(read_optional(&missing)?, None);
        Ok(())
    }
}
