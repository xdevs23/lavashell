// SPDX-License-Identifier: GPL-3.0-only
//! Reads that allocate only what a reader that charges memory has charged by then. A sequence goes into a buffer of
//! exactly its length, allocated after its first element. A value that goes into a `Box`, an `Arc` or the storage
//! of a private text is read inside a newtype struct named for the allocation, so the reader charges the allocation
//! before the read makes it.

use crate::protected_text::PrivateText;
use serde::de::{self, Deserialize, Deserializer, SeqAccess, Visitor};
use serde::{Serialize, Serializer};
use std::alloc::Layout;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

/// Reads a sequence into a buffer of exactly its length when the format gives the length, and otherwise grows the
/// buffer as the elements arrive.
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
        let Some(first) = elements.next_element()? else {
            return Ok(Vec::new());
        };
        let rest = elements.size_hint().unwrap_or(0);
        let mut sequence = Vec::with_capacity(rest.saturating_add(1));
        sequence.push(first);
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

/// An allocation that a read makes for what it reads inside a newtype struct with the name of the allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allocation {
    /// The value goes into a `Box`.
    Boxed,
    /// The elements of a sequence or the bytes of a string go into an `Arc` slice, copied from the buffer that they
    /// are read into.
    SharedSlice,
    /// The bytes of a byte string go into the storage of a private text, which an `Arc` shares.
    PrivateText,
}

const BOXED: &str = "lavashell::Boxed";
const SHARED_SLICE: &str = "lavashell::SharedSlice";
const PRIVATE_TEXT: &str = "lavashell::PrivateText";

impl Allocation {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Boxed => BOXED,
            Self::SharedSlice => SHARED_SLICE,
            Self::PrivateText => PRIVATE_TEXT,
        }
    }

    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            BOXED => Some(Self::Boxed),
            SHARED_SLICE => Some(Self::SharedSlice),
            PRIVATE_TEXT => Some(Self::PrivateText),
            _ => None,
        }
    }

    /// The bytes of the allocation that contains `read`: the layout of the value of a `Boxed` read, of the elements
    /// or the bytes of a `SharedSlice` read, or of the bytes of a `PrivateText` read. `None` when the allocation
    /// does not fit in memory.
    #[must_use]
    pub fn bytes(self, read: Layout) -> Option<usize> {
        match self {
            Self::Boxed => Some(read.size()),
            Self::SharedSlice => shared_bytes(read),
            Self::PrivateText => PrivateText::storage_bytes()?.checked_add(read.size()),
        }
    }
}

/// The bytes of an `Arc` that contains `data`. The strong and the weak count come before the data.
pub(crate) fn shared_bytes(data: Layout) -> Option<usize> {
    let (layout, _) = Layout::new::<[AtomicUsize; 2]>().extend(data).ok()?;
    Some(layout.pad_to_align().size())
}

/// A `Box` written and read as a `Boxed` newtype struct around its value.
pub(crate) mod boxed {
    use super::{
        Allocation, Deserialize, Deserializer, PhantomData, Serialize, Serializer, Visitor, fmt,
    };

    pub(crate) fn serialize<T: Serialize, S: Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(Allocation::Boxed.name(), value)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Box<T>, D::Error> {
        deserializer
            .deserialize_newtype_struct(Allocation::Boxed.name(), Unboxed(PhantomData))
            .map(Box::new)
    }

    struct Unboxed<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for Unboxed<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a value that goes into a box")
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<T, D::Error> {
            T::deserialize(deserializer)
        }
    }
}

/// An `Arc` slice written and read as a `SharedSlice` newtype struct around its elements.
pub(crate) mod shared_slice {
    use super::{
        Allocation, Arc, Deserialize, Deserializer, PhantomData, Serialize, Serializer, Visitor,
        fmt, sequence,
    };

    pub(crate) fn serialize<T: Serialize + ?Sized, S: Serializer>(
        shared: &Arc<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(Allocation::SharedSlice.name(), &**shared)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Arc<[T]>, D::Error> {
        let elements = SharedElements(PhantomData);
        deserializer.deserialize_newtype_struct(Allocation::SharedSlice.name(), elements)
    }

    struct SharedElements<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for SharedElements<T> {
        type Value = Arc<[T]>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a sequence that goes into a shared slice")
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Arc<[T]>, D::Error> {
            sequence(deserializer).map(Arc::from)
        }
    }
}

/// An `Arc` string written and read as a `SharedSlice` newtype struct around its text.
pub(crate) mod shared_str {
    pub(crate) use super::shared_slice::serialize;
    use super::{Allocation, Arc, Deserializer, Visitor, de, fmt};

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<str>, D::Error> {
        deserializer.deserialize_newtype_struct(Allocation::SharedSlice.name(), SharedText)
    }

    struct SharedText;

    impl<'de> Visitor<'de> for SharedText {
        type Value = Arc<str>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string that goes into a shared string")
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Arc<str>, D::Error> {
            deserializer.deserialize_str(self)
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

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Shared {
        #[serde(with = "shared_slice")]
        numbers: Arc<[u32]>,
        #[serde(with = "shared_str")]
        text: Arc<str>,
        #[serde(with = "boxed")]
        boxed: Box<u8>,
        #[serde(deserialize_with = "optional_sequence")]
        missing: Option<Vec<u8>>,
    }

    #[test]
    fn shared_and_boxed_values_read_back_as_written() -> TestResult {
        let written = Shared {
            numbers: Arc::from([1, 2, 3]),
            text: Arc::from("text"),
            boxed: Box::new(7),
            missing: None,
        };
        let bytes = postcard::to_allocvec(&written)?;
        assert_eq!(postcard::from_bytes::<Shared>(&bytes)?, written);
        Ok(())
    }

    #[test]
    fn each_allocation_is_found_by_its_name() {
        for allocation in [
            Allocation::Boxed,
            Allocation::SharedSlice,
            Allocation::PrivateText,
        ] {
            assert_eq!(Allocation::named(allocation.name()), Some(allocation));
        }
        assert_eq!(Allocation::named("Boxed"), None);
    }

    #[test]
    fn a_shared_slice_is_charged_for_its_counts_and_its_elements() {
        let counts = 2 * size_of::<usize>();
        let elements = Layout::new::<[u64; 3]>();
        assert_eq!(Allocation::SharedSlice.bytes(elements), Some(counts + 24));
        assert_eq!(Allocation::Boxed.bytes(Layout::new::<[u8; 5]>()), Some(5));
    }
}
