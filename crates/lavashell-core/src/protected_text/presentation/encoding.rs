// SPDX-License-Identifier: GPL-3.0-only
//! The secret of a private text is written as its UTF-8 bytes in one byte string, inside a newtype struct named for
//! the `PrivateText` allocation. A read copies the bytes straight into the storage of a `SecretText`, and an error
//! contains neither the bytes nor the text.

use super::super::SecretText;
use super::PrivateText;
use crate::encoding::{Allocation, shared_bytes};
use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserializer, Serialize, Serializer};
use std::alloc::Layout;
use std::{fmt, sync::Arc};
use zeroize::{Zeroize, Zeroizing};

impl PrivateText {
    /// The bytes that a read private text allocates besides the bytes of its text: the `Arc` that shares the secret.
    pub(crate) fn storage_bytes() -> Option<usize> {
        shared_bytes(Layout::new::<SecretText>())
    }
}

pub(super) fn serialize<S: Serializer>(
    secret: &Arc<SecretText>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let name = Allocation::PrivateText.name();
    secret.expose(|text| serializer.serialize_newtype_struct(name, &Plaintext(text.as_bytes())))
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Arc<SecretText>, D::Error> {
    deserializer.deserialize_newtype_struct(Allocation::PrivateText.name(), PlaintextVisitor)
}

struct Plaintext<'a>(&'a [u8]);

impl Serialize for Plaintext<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

struct PlaintextVisitor;

impl PlaintextVisitor {
    fn not_utf8<E: Error>(&self) -> E {
        E::invalid_value(Unexpected::Other("bytes that are not UTF-8"), self)
    }
}

impl<'de> Visitor<'de> for PlaintextVisitor {
    type Value = Arc<SecretText>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the UTF-8 bytes of a private text")
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Arc<SecretText>, D::Error> {
        deserializer.deserialize_bytes(self)
    }

    fn visit_bytes<E: Error>(self, bytes: &[u8]) -> Result<Arc<SecretText>, E> {
        match std::str::from_utf8(bytes) {
            Ok(text) => Ok(secret(text.to_owned())),
            Err(_) => Err(self.not_utf8()),
        }
    }

    fn visit_byte_buf<E: Error>(self, bytes: Vec<u8>) -> Result<Arc<SecretText>, E> {
        match String::from_utf8(bytes) {
            Ok(text) => Ok(secret(text)),
            Err(error) => {
                error.into_bytes().zeroize();
                Err(self.not_utf8())
            }
        }
    }
}

fn secret(text: String) -> Arc<SecretText> {
    Arc::new(SecretText {
        text: Zeroizing::new(text),
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::ProtectedText;
    use super::PrivateText;
    use crate::test_error::{Required, TestResult};

    #[test]
    fn a_private_text_reads_back_as_written() -> TestResult {
        let field = ProtectedText::new();
        assert_eq!(field.replace(0..0, "hunter2 ✓"), 9);
        field.set_revealed(true);
        let shown = field.presentation('•').required("a revealed text")?;
        let bytes = postcard::to_allocvec(&shown)?;
        let read: PrivateText = postcard::from_bytes(&bytes)?;
        assert_eq!(read.expose(str::to_owned), "hunter2 ✓");
        assert_eq!(read.mask, '•');
        Ok(())
    }

    #[test]
    fn bytes_that_are_not_utf8_are_not_read() -> TestResult {
        let bytes = postcard::to_allocvec(&([0xff_u8, 0xfe].as_slice(), '•'))?;
        assert!(postcard::from_bytes::<PrivateText>(&bytes).is_err());
        Ok(())
    }
}
