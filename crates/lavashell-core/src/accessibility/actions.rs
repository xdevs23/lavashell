// SPDX-License-Identifier: GPL-3.0-only
use super::{AccessibleValue, TextSelection};
use crate::geometry::Axis;
use crate::ui::NodeId;
use crate::units::{Dp, NumberError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ActionKind {
    Focus,
    Activate,
    SetValue,
    Increment,
    Decrement,
    /// A scroll along the given axis, offered by a node that scrolls along that axis.
    Scroll(Axis),
    ScrollIntoView,
    SetTextSelection,
    ReplaceSelectedText,
    RevealCollectionItem,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AccessibilityAction {
    Focus,
    Activate,
    SetValue(AccessibleValue),
    Increment,
    Decrement,
    /// A scroll by a distance along an axis on which the node scrolls. A positive distance moves toward the end.
    Scroll(Axis, Dp),
    ScrollIntoView,
    SetTextSelection(TextSelection),
    ReplaceSelectedText(String),
    /// Brings the item at `row` and `column` of a collection into view through the handler registered by the
    /// collection. A recycled list composes the item in a later frame.
    RevealCollectionItem {
        row: usize,
        column: usize,
    },
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
            _ => true,
        }
    }
}

/// An accessibility action on the node `target`, sent from a platform worker thread to the UI thread.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionRequest {
    pub target: NodeId,
    pub action: AccessibilityAction,
}

/// The result of handling an `ActionRequest`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    /// The handler of the node or the default behavior of the view performed the action.
    Performed,
    /// Neither a handler nor a default behavior performed the action. The target no longer exists, is
    /// excluded or disabled, or does not offer the action, or the request is malformed.
    Ignored,
}

/// The error returned by a handler that could not perform the requested action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("a handler cannot count the items of a collection exactly")]
    Count(#[source] NumberError),
}
