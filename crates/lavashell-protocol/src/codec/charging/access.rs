// SPDX-License-Identifier: GPL-3.0-only
//! The visitor, sequence, variant and seed wrappers that pass the charging deserializer on to every value that
//! the decode reads.

use super::{Charging, Failure, PendingCharges, Storage, buffer_bytes};
use crate::Uncharged;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};
use std::fmt;

pub(super) struct ChargingVisitor<'b, V> {
    pub(super) inner: V,
    pub(super) charges: &'b PendingCharges,
    pub(super) storage: Storage,
}

impl<V> ChargingVisitor<'_, V> {
    fn shared(&self) -> bool {
        matches!(self.storage, Storage::Buffer { shared: true })
    }
}

macro_rules! forward_values {
    ($($method:ident($value:ty)),* $(,)?) => {$(
        fn $method<E: de::Error>(self, value: $value) -> Result<V::Value, E> {
            self.inner.$method(value)
        }
    )*};
}

macro_rules! charge_lengths {
    ($($method:ident($value:ty)),* $(,)?) => {$(
        fn $method<E: de::Error>(self, value: $value) -> Result<V::Value, E> {
            self.charges.charge(buffer_bytes::<u8>(self.shared(), value.len()))?;
            self.inner.$method(value)
        }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for ChargingVisitor<'_, V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(formatter)
    }

    forward_values!(
        visit_bool(bool),
        visit_i8(i8),
        visit_i16(i16),
        visit_i32(i32),
        visit_i64(i64),
        visit_i128(i128),
        visit_u8(u8),
        visit_u16(u16),
        visit_u32(u32),
        visit_u64(u64),
        visit_u128(u128),
        visit_f32(f32),
        visit_f64(f64),
        visit_char(char),
    );

    charge_lengths!(
        visit_str(&str),
        visit_borrowed_str(&'de str),
        visit_string(String),
        visit_bytes(&[u8]),
        visit_borrowed_bytes(&'de [u8]),
        visit_byte_buf(Vec<u8>),
    );

    fn visit_none<E: de::Error>(self) -> Result<V::Value, E> {
        self.inner.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<V::Value, E> {
        self.inner.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<V::Value, D::Error> {
        self.inner
            .visit_some(Charging::new(deserializer, self.charges))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<V::Value, D::Error> {
        let shared = self.shared();
        self.inner.visit_newtype_struct(Charging {
            inner: deserializer,
            charges: self.charges,
            shared,
        })
    }

    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<V::Value, A::Error> {
        let elements = match self.storage {
            Storage::Inline => Elements::Inline,
            Storage::Buffer { shared } => Elements::Uncharged {
                length: (sequence.size_hint())
                    .ok_or_else(|| self.charges.fail(Failure::Uncharged(Uncharged::Length)))?,
                shared,
            },
        };
        self.inner.visit_seq(ChargingElements {
            inner: sequence,
            charges: self.charges,
            elements,
        })
    }

    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<V::Value, A::Error> {
        Err(self.charges.fail(Failure::Uncharged(Uncharged::Map)))
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<V::Value, A::Error> {
        self.inner.visit_enum(ChargingVariant {
            inner: data,
            charges: self.charges,
        })
    }
}

/// The storage of the elements of a sequence, and whether their buffer is charged yet.
#[derive(Clone, Copy)]
enum Elements {
    /// Inside the value that contains the sequence.
    Inline,
    /// In a buffer of `length` elements, charged with the first element, when the first element shows their type.
    Uncharged { length: usize, shared: bool },
    /// In a charged buffer, of which `unread` elements are left.
    Charged { unread: usize },
}

struct ChargingElements<'b, A> {
    inner: A,
    charges: &'b PendingCharges,
    elements: Elements,
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for ChargingElements<'_, A> {
    type Error = A::Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        let seed = ChargingSeed {
            inner: seed,
            charges: self.charges,
        };
        let unread = match self.elements {
            Elements::Inline => return self.inner.next_element_seed(seed),
            Elements::Uncharged { length, shared } => {
                self.charges
                    .charge(buffer_bytes::<S::Value>(shared, length))?;
                length
            }
            Elements::Charged { unread } => unread,
        };
        let element = self.inner.next_element_seed(seed)?;
        let unread = match element {
            Some(_) => unread
                .checked_sub(1)
                .ok_or_else(|| self.charges.fail(Failure::Uncharged(Uncharged::Elements)))?,
            None => unread,
        };
        self.elements = Elements::Charged { unread };
        Ok(element)
    }

    /// No length before the buffer is charged, so a collection reserves no memory that is not charged.
    fn size_hint(&self) -> Option<usize> {
        match self.elements {
            Elements::Inline => self.inner.size_hint(),
            Elements::Uncharged { .. } => None,
            Elements::Charged { unread } => Some(unread),
        }
    }
}

struct ChargingVariant<'b, A> {
    inner: A,
    charges: &'b PendingCharges,
}

impl<'b, A> ChargingVariant<'b, A> {
    fn seed<S>(&self, inner: S) -> ChargingSeed<'b, S> {
        ChargingSeed {
            inner,
            charges: self.charges,
        }
    }

    fn visitor<V>(&self, inner: V) -> ChargingVisitor<'b, V> {
        ChargingVisitor {
            inner,
            charges: self.charges,
            storage: Storage::Inline,
        }
    }
}

impl<'b, 'de, A: EnumAccess<'de>> EnumAccess<'de> for ChargingVariant<'b, A> {
    type Error = A::Error;
    type Variant = ChargingVariant<'b, A::Variant>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), A::Error> {
        let seed = self.seed(seed);
        let (value, variant) = self.inner.variant_seed(seed)?;
        Ok((
            value,
            ChargingVariant {
                inner: variant,
                charges: self.charges,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for ChargingVariant<'_, A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), A::Error> {
        self.inner.unit_variant()
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, A::Error> {
        let seed = self.seed(seed);
        self.inner.newtype_variant_seed(seed)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        let visitor = self.visitor(visitor);
        self.inner.tuple_variant(len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        let visitor = self.visitor(visitor);
        self.inner.struct_variant(fields, visitor)
    }
}

struct ChargingSeed<'b, S> {
    inner: S,
    charges: &'b PendingCharges,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for ChargingSeed<'_, S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<S::Value, D::Error> {
        self.inner
            .deserialize(Charging::new(deserializer, self.charges))
    }
}
