// SPDX-License-Identifier: GPL-3.0-only
use serde::de::{self, Deserializer, Unexpected};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedFlag<const VALUE: bool>;

impl<const VALUE: bool> Serialize for FixedFlag<VALUE> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(VALUE)
    }
}

impl<'de, const VALUE: bool> Deserialize<'de> for FixedFlag<VALUE> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = bool::deserialize(deserializer)?;
        if value == VALUE {
            Ok(Self)
        } else {
            Err(de::Error::invalid_value(
                Unexpected::Bool(value),
                &if VALUE { "true" } else { "false" },
            ))
        }
    }
}
