// SPDX-License-Identifier: GPL-3.0-only
//! Messages are postcard bytes. A decode charges an account before each allocation of the decoded value: a sequence
//! for its whole buffer before its first element, each string and byte string by its length, and each `Box` and
//! `Arc` that a read makes, while the buffer that it copies is still alive. A message that needs more than the
//! account has left fails before it allocates past the account.

mod charging;
#[cfg(test)]
mod tests;

use crate::ProtocolError;
use charging::{Charging, Failure, PendingCharges};
use postcard::ser_flavors::Size;
use serde::{Serialize, de::DeserializeOwned};
use zeroize::Zeroizing;

/// The memory that the decoded values of a sender may still use.
#[derive(Debug, PartialEq, Eq)]
pub struct Account {
    remaining: usize,
}

impl Account {
    #[must_use]
    pub fn new(bytes: usize) -> Self {
        Self { remaining: bytes }
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Gives back the memory of a decoded value once the value is dropped.
    pub fn release(&mut self, Charge { bytes }: Charge) {
        self.remaining = self.remaining.saturating_add(bytes);
    }
}

/// A value whose memory a decode cannot charge before the value allocates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Uncharged {
    #[error("a sequence whose format does not give its length before its elements")]
    Length,
    #[error("a sequence with more elements than its length")]
    Elements,
    #[error("a map")]
    Map,
    #[error("a sequence or a box larger than memory")]
    Size,
}

/// The memory charged to an account for one decoded message.
#[derive(Debug, PartialEq, Eq)]
pub struct Charge {
    bytes: usize,
}

impl Charge {
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

/// A decoded message and the memory charged to the account for it.
#[derive(Debug)]
pub struct Decoded<T> {
    pub value: T,
    pub charge: Charge,
}

/// The bytes of an encoded message, overwritten with zeros when they are dropped, so the bytes of a private text
/// leave no copy behind.
pub struct Encoded(Zeroizing<Vec<u8>>);

impl Encoded {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Writes into a buffer of the exact size of the message, which never grows and so never leaves a copy in memory
/// that it gave back.
///
/// # Errors
///
/// `ProtocolError::Encode` when the value cannot be written, such as a value with an id that is valid only in its
/// process.
pub fn encode(value: &impl Serialize) -> Result<Encoded, ProtocolError> {
    let size =
        postcard::serialize_with_flavor(value, Size::default()).map_err(ProtocolError::Encode)?;
    let mut bytes = Zeroizing::new(vec![0; size]);
    let written = postcard::to_slice(value, bytes.as_mut_slice())
        .map_err(ProtocolError::Encode)?
        .len();
    bytes.truncate(written);
    Ok(Encoded(bytes))
}

/// Decodes one whole message and charges `account` for the memory of the value. A message that fails to decode
/// charges nothing.
///
/// # Errors
///
/// `ProtocolError::Charge` when the value needs more memory than `account` has left, `ProtocolError::Uncharged` when
/// the value contains a part whose memory cannot be charged before it is allocated, `ProtocolError::Decode` when the
/// bytes are not a value of `T`, and `ProtocolError::Finish` when bytes follow the end of the value.
pub fn decode<T: DeserializeOwned>(
    bytes: &[u8],
    account: &mut Account,
) -> Result<Decoded<T>, ProtocolError> {
    let charges = PendingCharges::new(account.remaining);
    let mut input = postcard::Deserializer::from_bytes(bytes);
    let value =
        T::deserialize(Charging::new(&mut input, &charges)).map_err(|source| {
            match charges.failure() {
                Some(Failure::Charge { requested }) => ProtocolError::Charge {
                    requested,
                    remaining: charges.remaining(),
                },
                Some(Failure::Uncharged(part)) => ProtocolError::Uncharged(part),
                None => ProtocolError::Decode(source),
            }
        })?;
    let trailing = input.finalize().map_err(ProtocolError::Decode)?.len();
    if trailing > 0 {
        return Err(ProtocolError::Finish { trailing });
    }
    let charge = Charge {
        bytes: account.remaining - charges.remaining(),
    };
    account.remaining = charges.remaining();
    Ok(Decoded { value, charge })
}
