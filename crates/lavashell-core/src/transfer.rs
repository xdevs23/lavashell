// SPDX-License-Identifier: GPL-3.0-only

use crate::flag_set::flag_set;
use crate::geometry::Point;
use crate::surface::SurfaceId;
use lavashell_enum_values::enum_with_values;
use std::cmp::Reverse;
use std::sync::Arc;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TextFormat {
        Utf8,
        Plain,
    }

    const ALL;
}

impl TextFormat {
    #[must_use]
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Utf8 => "text/plain;charset=utf-8",
            Self::Plain => "text/plain",
        }
    }

    fn preference(self) -> u8 {
        match self {
            Self::Plain => 0,
            Self::Utf8 => 1,
        }
    }

    pub fn by_preference() -> impl Iterator<Item = Self> {
        let mut formats = Self::ALL;
        formats.sort_unstable_by_key(|format| Reverse(format.preference()));
        formats.into_iter()
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferData(Vec<Representation>);

impl TransferData {
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

    pub fn single(
        mime_type: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, TransferError> {
        Self::new([Representation::new(mime_type, bytes)?])
    }

    #[must_use]
    pub fn text(text: &str) -> Self {
        let bytes: Arc<[u8]> = text.as_bytes().into();
        Self(
            TextFormat::by_preference()
                .map(|format| Representation {
                    mime_type: format.mime_type().to_owned(),
                    bytes: bytes.clone(),
                })
                .collect(),
        )
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

flag_set!(pub enum DragAction in DragActions { Copy, Move });

impl DragActions {
    #[must_use]
    pub fn preferred(self) -> Option<DragAction> {
        self.iter().max_by_key(|action| action.preference())
    }
}

impl DragAction {
    fn preference(self) -> u8 {
        match self {
            Self::Copy => 0,
            Self::Move => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Cancellation {
    #[error("the app closed")]
    AppClosed,
    #[error("the input device of the transfer disappeared")]
    InputDeviceGone,
    #[error("the surface of the transfer closed")]
    SurfaceClosed,
    #[error("the host system cancelled the data source")]
    SourceCancelled,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferId(pub u64);

#[derive(Clone, Debug)]
pub struct DragOfferInfo {
    pub id: TransferId,
    pub surface: SurfaceId,
    pub position: Point,
    pub mime_types: Vec<String>,
    pub actions: DragActions,
    pub selected_action: Option<DragAction>,
}

#[derive(Clone, Debug)]
pub enum DragEvent {
    Enter(DragOfferInfo),
    Motion(DragOfferInfo),
    Drop(DragOfferInfo),
    Leave { id: TransferId, surface: SurfaceId },
    Cancelled { id: TransferId, surface: SurfaceId },
}

impl DragEvent {
    #[must_use]
    pub fn id(&self) -> TransferId {
        match self {
            Self::Enter(info) | Self::Motion(info) | Self::Drop(info) => info.id,
            Self::Leave { id, .. } | Self::Cancelled { id, .. } => *id,
        }
    }

    #[must_use]
    pub fn surface(&self) -> SurfaceId {
        match self {
            Self::Enter(info) | Self::Motion(info) | Self::Drop(info) => info.surface,
            Self::Leave { surface, .. } | Self::Cancelled { surface, .. } => *surface,
        }
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
    fn text_is_offered_as_utf8_in_every_text_format() -> TestResult {
        let data = TransferData::text("雪🙂");
        let formats = ["text/plain;charset=utf-8", "text/plain"];
        assert_eq!(data.formats().collect::<Vec<_>>(), formats);
        for format in formats {
            assert_eq!(
                &**data.get(format).required("text bytes")?,
                "雪🙂".as_bytes()
            );
        }
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

    #[test]
    fn a_move_is_preferred_over_a_copy_when_both_are_allowed() {
        let both = DragActions::all();
        assert_eq!(both.preferred(), Some(DragAction::Move));
        assert_eq!(
            DragActions::new([DragAction::Copy]).preferred(),
            Some(DragAction::Copy)
        );
        assert_eq!(DragActions::default().preferred(), None);
    }
}
