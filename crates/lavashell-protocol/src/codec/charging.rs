// SPDX-License-Identifier: GPL-3.0-only
//! A deserializer that charges the account before the decoded value allocates: a sequence for its whole buffer before
//! its first element, each string and byte string by its length, and each newtype struct named for an `Allocation`
//! for that allocation. A map, and a sequence whose length the format does not give up front, fail the decode.

mod access;

use super::Uncharged;
use access::ChargingVisitor;
use lavashell_core::encoding::Allocation;
use serde::de::{self, Deserializer, Visitor};
use std::alloc::Layout;
use std::cell::Cell;

/// Why a decode stopped before it allocated.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(super) enum Failure {
    #[error("the decoded message needs {requested} bytes, more than the account has left")]
    Charge { requested: usize },
    #[error(transparent)]
    Uncharged(Uncharged),
}

pub(super) struct PendingCharges {
    remaining: Cell<usize>,
    failure: Cell<Option<Failure>>,
}

impl PendingCharges {
    pub(super) fn new(remaining: usize) -> Self {
        Self {
            remaining: Cell::new(remaining),
            failure: Cell::new(None),
        }
    }

    pub(super) fn remaining(&self) -> usize {
        self.remaining.get()
    }

    /// The failure that ended the decode.
    pub(super) fn failure(&self) -> Option<Failure> {
        self.failure.get()
    }

    /// `bytes` is `None` for an allocation larger than memory.
    fn charge<E: de::Error>(&self, bytes: Option<usize>) -> Result<(), E> {
        let bytes = bytes.ok_or_else(|| self.fail(Failure::Uncharged(Uncharged::Size)))?;
        let remaining = (self.remaining.get().checked_sub(bytes))
            .ok_or_else(|| self.fail(Failure::Charge { requested: bytes }))?;
        self.remaining.set(remaining);
        Ok(())
    }

    fn fail<E: de::Error>(&self, failure: Failure) -> E {
        self.failure.set(Some(failure));
        E::custom(failure)
    }
}

/// The bytes of a buffer of `length` elements of `T`, with its copy into an `Arc` slice when it is `shared`.
fn buffer_bytes<T>(shared: bool, length: usize) -> Option<usize> {
    let buffer = Layout::array::<T>(length).ok()?;
    if shared {
        buffer
            .size()
            .checked_add(Allocation::SharedSlice.bytes(buffer)?)
    } else {
        Some(buffer.size())
    }
}

/// Where the contents that a visitor receives are stored: the elements of a sequence, the bytes of a string, or the
/// value inside a newtype struct.
#[derive(Clone, Copy)]
enum Storage {
    /// Inside the value that contains them.
    Inline,
    /// In a buffer of their own, copied into an `Arc` slice once read when the buffer is `shared`.
    Buffer { shared: bool },
}

pub(super) struct Charging<'b, D> {
    inner: D,
    charges: &'b PendingCharges,
    /// Whether the sequence or the string read next is copied into an `Arc` slice.
    shared: bool,
}

impl<'b, D> Charging<'b, D> {
    pub(super) fn new(inner: D, charges: &'b PendingCharges) -> Self {
        Self {
            inner,
            charges,
            shared: false,
        }
    }

    fn visitor<V>(&self, inner: V, storage: Storage) -> ChargingVisitor<'b, V> {
        ChargingVisitor {
            inner,
            charges: self.charges,
            storage,
        }
    }

    fn buffer(&self) -> Storage {
        Storage::Buffer {
            shared: self.shared,
        }
    }
}

macro_rules! forward_inline {
    ($($method:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            let visitor = self.visitor(visitor, Storage::Inline);
            self.inner.$method(visitor)
        }
    )*};
}

macro_rules! forward_buffer {
    ($($method:ident),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            let visitor = self.visitor(visitor, self.buffer());
            self.inner.$method(visitor)
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Charging<'_, D> {
    type Error = D::Error;

    forward_inline!(
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_i128,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_u128,
        deserialize_f32,
        deserialize_f64,
        deserialize_char,
        deserialize_option,
        deserialize_unit,
        deserialize_map,
        deserialize_identifier,
        deserialize_ignored_any,
    );

    forward_buffer!(
        deserialize_any,
        deserialize_str,
        deserialize_string,
        deserialize_bytes,
        deserialize_byte_buf,
        deserialize_seq,
    );

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.visitor(visitor, Storage::Inline);
        self.inner.deserialize_unit_struct(name, visitor)
    }

    /// Charges the allocation of a newtype struct named for one before the value is read, or marks the sequence or
    /// the string inside as shared.
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let storage = match Allocation::named(name) {
            Some(Allocation::SharedSlice) => Storage::Buffer { shared: true },
            Some(allocation) => {
                self.charges
                    .charge(allocation.bytes(Layout::new::<V::Value>()))?;
                Storage::Inline
            }
            None => Storage::Inline,
        };
        let visitor = self.visitor(visitor, storage);
        self.inner.deserialize_newtype_struct(name, visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.visitor(visitor, Storage::Inline);
        self.inner.deserialize_tuple(len, visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.visitor(visitor, Storage::Inline);
        self.inner.deserialize_tuple_struct(name, len, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.visitor(visitor, Storage::Inline);
        self.inner.deserialize_struct(name, fields, visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.visitor(visitor, Storage::Inline);
        self.inner.deserialize_enum(name, variants, visitor)
    }

    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}
