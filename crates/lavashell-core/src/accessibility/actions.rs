// SPDX-License-Identifier: GPL-3.0-only
use super::{AccessibleValue, TextSelection};
use crate::geometry::Axis;
use crate::ui::NodeId;
use crate::units::{Dp, NumberError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActionKind {
    Focus,
    Activate,
    SetValue,
    Increment,
    Decrement,
    Scroll(Axis),
    ScrollIntoView,
    SetTextSelection,
    ReplaceSelectedText,
    RevealCollectionItem,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AccessibilityAction {
    Focus,
    Activate,
    SetValue(AccessibleValue),
    Increment,
    Decrement,
    Scroll(Axis, Dp),
    ScrollIntoView,
    SetTextSelection(TextSelection),
    ReplaceSelectedText(String),
    RevealCollectionItem { row: usize, column: usize },
}

impl AccessibilityAction {
    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Focus => ActionKind::Focus,
            Self::Activate => ActionKind::Activate,
            Self::SetValue(_) => ActionKind::SetValue,
            Self::Increment => ActionKind::Increment,
            Self::Decrement => ActionKind::Decrement,
            Self::Scroll(axis, _) => ActionKind::Scroll(*axis),
            Self::ScrollIntoView => ActionKind::ScrollIntoView,
            Self::SetTextSelection(_) => ActionKind::SetTextSelection,
            Self::ReplaceSelectedText(_) => ActionKind::ReplaceSelectedText,
            Self::RevealCollectionItem { .. } => ActionKind::RevealCollectionItem,
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Self::SetValue(AccessibleValue::Number(value)) => value.is_finite(),
            Self::Scroll(_, delta) => delta.is_finite(),
            Self::Focus
            | Self::Activate
            | Self::SetValue(_)
            | Self::Increment
            | Self::Decrement
            | Self::ScrollIntoView
            | Self::SetTextSelection(_)
            | Self::ReplaceSelectedText(_)
            | Self::RevealCollectionItem { .. } => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionRequest {
    pub target: NodeId,
    pub action: AccessibilityAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    Performed,
    Ignored(IgnoredBecause),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IgnoredBecause {
    UnknownTarget,
    StaleTarget,
    Disabled,
    NotOffered,
    NotFinite,
    NoCollection,
    OutsideCollection,
    NoRange,
    NoStep,
    NoNumber,
    TextIntoRange,
    NotText,
    NotEditable,
    SelectionOutsideText,
    SelectionSplitsCluster,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("a handler cannot count the items of a collection exactly")]
    Count(#[source] NumberError),
}
