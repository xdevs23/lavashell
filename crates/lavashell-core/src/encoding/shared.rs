// SPDX-License-Identifier: GPL-3.0-only
//! The serde forms for shared slices and shared strings.

/// An `Arc` slice written as its elements and read through `sequence`.
pub(crate) mod shared_slice {
    use super::super::sequence;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub(crate) fn serialize<T: Serialize + ?Sized, S: Serializer>(
        shared: &Arc<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        (**shared).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Arc<[T]>, D::Error> {
        sequence(deserializer).map(Arc::from)
    }
}

/// An `Arc` string written and read as its text.
pub(crate) mod shared_str {
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serialize, Serializer};
    use std::{fmt, sync::Arc};

    pub(crate) fn serialize<T: Serialize + ?Sized, S: Serializer>(
        shared: &Arc<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        (**shared).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<str>, D::Error> {
        deserializer.deserialize_str(SharedText)
    }

    struct SharedText;

    impl Visitor<'_> for SharedText {
        type Value = Arc<str>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string that goes into a shared string")
        }

        fn visit_str<E: de::Error>(self, text: &str) -> Result<Arc<str>, E> {
            Ok(Arc::from(text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestResult;
    use std::sync::Arc;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Shared {
        #[serde(with = "shared_slice")]
        numbers: Arc<[u32]>,
        #[serde(with = "shared_str")]
        text: Arc<str>,
    }

    fn shared() -> Shared {
        Shared {
            numbers: Arc::from([1, 2, 3]),
            text: Arc::from("text"),
        }
    }

    #[test]
    fn shared_values_read_back_as_written() -> TestResult {
        let bytes = postcard::to_allocvec(&shared())?;
        assert_eq!(postcard::from_bytes::<Shared>(&bytes)?, shared());
        Ok(())
    }

    #[test]
    fn shared_values_are_written_as_a_plain_sequence_and_text() -> TestResult {
        let plain = postcard::to_allocvec(&(vec![1_u32, 2, 3], "text"))?;
        assert_eq!(postcard::to_allocvec(&shared())?, plain);
        Ok(())
    }
}
