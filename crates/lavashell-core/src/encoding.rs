// SPDX-License-Identifier: GPL-3.0-only
//! The serde forms of the values of core. A value that a constructor checks is read through that check. A value
//! that is valid only in the process that made it fails to be written and to be read.

mod allocation;

pub use allocation::{Allocation, sequence};
pub(crate) use allocation::{boxed, optional_sequence, shared_bytes, shared_slice, shared_str};

/// Writes each checked number as its value and reads it through its `try_new`.
macro_rules! checked_serde {
    ($($name:ident($value:ty)),* $(,)?) => {$(
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serde::Serialize::serialize(&self.get(), serializer)
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <$value as serde::Deserialize>::deserialize(deserializer)?;
                Self::try_new(value).map_err(serde::de::Error::custom)
            }
        }
    )*};
}
pub(crate) use checked_serde;

/// The serde form of a field whose value is valid only in the process that made it.
pub(crate) mod process_local {
    use serde::{Deserializer, Serializer, de, ser};

    const LOCAL: &str = "the value is valid only in the process that made it";

    pub(crate) fn serialize<T, S: Serializer>(_: &T, _: S) -> Result<S::Ok, S::Error> {
        Err(ser::Error::custom(LOCAL))
    }

    pub(crate) fn deserialize<'de, T, D: Deserializer<'de>>(_: D) -> Result<T, D::Error> {
        Err(de::Error::custom(LOCAL))
    }
}
