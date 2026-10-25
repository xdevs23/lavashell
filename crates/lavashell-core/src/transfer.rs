// SPDX-License-Identifier: GPL-3.0-only
//! Clipboard and drag-and-drop data in a form independent of the platform. An item has one or more representations,
//! each a MIME type with bytes. A drop acceptance is the format and the action chosen by the receiving application.

use std::sync::Arc;

/// Why a representation or a transfer cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    #[error("a transfer format must be nonempty and contain no control characters")]
    Format,
    #[error("a transfer needs at least one representation")]
    NoRepresentation,
    #[error("the formats of a transfer must be unique")]
    DuplicateFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Representation {
    mime_type: String,
    bytes: Arc<[u8]>,
}

impl Representation {
    /// # Errors
    ///
    /// Fails when `mime_type` is empty or contains a control character.
    pub fn new(
        mime_type: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, TransferError> {
        let mime_type = mime_type.into();
        if mime_type.is_empty() || mime_type.chars().any(char::is_control) {
            return Err(TransferError::Format);
        }
        Ok(Self {
            mime_type,
            bytes: bytes.into(),
        })
    }

    #[must_use]
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    #[must_use]
    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
}

/// One item of a transfer as one or more equivalent representations, in the order preferred by the sending
/// application. A file reference is sent as the bytes of a URI list given by the application. The transfer
/// never opens the referenced files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferData(Vec<Representation>);

impl TransferData {
    /// # Errors
    ///
    /// Fails when `representations` is empty or has two representations with the same MIME type.
    pub fn new(
        representations: impl IntoIterator<Item = Representation>,
    ) -> Result<Self, TransferError> {
        let representations: Vec<_> = representations.into_iter().collect();
        if representations.is_empty() {
            return Err(TransferError::NoRepresentation);
        }
        for (index, item) in representations.iter().enumerate() {
            if representations[..index]
                .iter()
                .any(|prior| prior.mime_type == item.mime_type)
            {
                return Err(TransferError::DuplicateFormat);
            }
        }
        Ok(Self(representations))
    }

    /// # Errors
    ///
    /// Fails when `mime_type` is empty or contains a control character.
    pub fn single(
        mime_type: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, TransferError> {
        Self::new([Representation::new(mime_type, bytes)?])
    }

    pub fn formats(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(Representation::mime_type)
    }

    pub fn get(&self, mime_type: &str) -> Option<&Arc<[u8]>> {
        self.0
            .iter()
            .find(|item| item.mime_type == mime_type)
            .map(Representation::bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedData {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAction {
    Copy,
    Move,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragActions(u8);

impl DragActions {
    pub fn new(actions: impl IntoIterator<Item = DragAction>) -> Self {
        Self(
            actions
                .into_iter()
                .fold(0, |bits, action| bits | action.bit()),
        )
    }

    #[must_use]
    pub fn contains(self, action: DragAction) -> bool {
        self.0 & action.bit() != 0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl DragAction {
    fn bit(self) -> u8 {
        match self {
            Self::Copy => 1,
            Self::Move => 2,
        }
    }
}

/// The MIME type and the action with which the receiving application accepts a drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropAcceptance {
    pub mime_type: String,
    pub action: DragAction,
}

impl DropAcceptance {
    #[must_use]
    pub fn accepts(&self, formats: &[String], actions: DragActions) -> bool {
        formats.contains(&self.mime_type) && actions.contains(self.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::{Required, TestResult};

    #[test]
    fn formats_preserve_order_and_binary_bytes_without_conversion() -> TestResult {
        let data = TransferData::new([
            Representation::new("application/x-custom", vec![0, 255, 1])?,
            Representation::new("text/plain", b"alternate".as_slice())?,
        ])?;
        assert_eq!(
            data.formats().collect::<Vec<_>>(),
            ["application/x-custom", "text/plain"]
        );
        assert_eq!(
            &**data.get("application/x-custom").required("custom bytes")?,
            [0, 255, 1]
        );
        assert!(data.get("image/png").is_none());
        Ok(())
    }

    #[test]
    fn empty_duplicate_and_control_character_formats_are_rejected() -> TestResult {
        assert!(TransferData::new([]).is_err());
        assert!(Representation::new("bad\0format", vec![]).is_err());
        let item = Representation::new("text/plain", vec![])?;
        assert!(TransferData::new([item.clone(), item]).is_err());
        Ok(())
    }

    #[test]
    fn unsupported_format_or_action_cannot_be_accepted() {
        let choice = DropAcceptance {
            mime_type: "image/png".into(),
            action: DragAction::Move,
        };
        let formats = vec!["image/png".into()];
        assert!(!choice.accepts(&formats, DragActions::new([DragAction::Copy])));
        assert!(!choice.accepts(&[], DragActions::new([DragAction::Move])));
        assert!(choice.accepts(&formats, DragActions::new([DragAction::Move])));
    }
}
