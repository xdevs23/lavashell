// SPDX-License-Identifier: GPL-3.0-only
//! The failures that accessibility tests return in place of a panic.

use accesskit::{NodeId as NativeId, Role as NativeRole};
use lavashell_core::prelude::NodeId;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AccessibilityTestError {
    #[error("the platform could not show a frame")]
    NotShown,
    #[error("the runtime did not publish a tree for the surface")]
    NotPublished,
    #[error("the published tree has no button")]
    NoButton,
    #[error("the tree has no node {0:?}")]
    MissingNode(NodeId),
    #[error("the node {0:?} has no text")]
    MissingText(NodeId),
    #[error("the native tree has no node with the role {0:?}")]
    MissingNativeRole(NativeRole),
    #[error("the native tree has no node {0:?}")]
    MissingNativeNode(NativeId),
    #[error("the native tree has no tree information")]
    MissingTreeInfo,
    #[error("the native node has no text selection")]
    MissingSelection,
    #[error("the action request does not map to an action of Lavashell")]
    RefusedAction,
    #[error("the native text has no position at index {0}")]
    MissingPosition(usize),
    #[error("the native text has no line {0}")]
    MissingLine(usize),
}
