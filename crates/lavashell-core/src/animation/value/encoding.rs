// SPDX-License-Identifier: GPL-3.0-only
//! A vector is written as a tuple of its components.

use super::Vector;
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

impl<const N: usize> Serialize for Vector<N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(N)?;
        (self.0.iter()).try_for_each(|component| tuple.serialize_element(component))?;
        tuple.end()
    }
}

impl<'de, const N: usize> Deserialize<'de> for Vector<N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_tuple(N, Components::<N>)
    }
}

struct Components<const N: usize>;

impl<'de, const N: usize> Visitor<'de> for Components<N> {
    type Value = Vector<N>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{N} components")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vector<N>, A::Error> {
        let mut components = [0.0; N];
        for (index, component) in components.iter_mut().enumerate() {
            *component = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(index, &self))?;
        }
        Ok(Vector(components))
    }
}

#[cfg(test)]
mod tests {
    use super::Vector;
    use crate::test_error::TestResult;

    #[test]
    fn a_vector_is_written_as_its_components() -> TestResult {
        let vector = Vector([0.5, -2.0]);
        let bytes = postcard::to_allocvec(&vector)?;
        assert_eq!(bytes, postcard::to_allocvec(&(0.5_f64, -2.0_f64))?);
        assert_eq!(postcard::from_bytes::<Vector<2>>(&bytes)?, vector);
        Ok(())
    }
}
