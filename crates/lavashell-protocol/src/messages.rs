// SPDX-License-Identifier: GPL-3.0-only
//! The messages of `VERSION` after the hello. Times are on the monotonic clock of the system.

use crate::{AccessibilityUpdate, NonNegativeSize};
use lavashell_core::accessibility::ActionRequest;
use lavashell_core::effects::{EffectId, SourceFilterId};
use lavashell_core::encoding::sequence;
use lavashell_core::input_method::{TextCommand, TextSession};
use lavashell_core::preferences::PreferenceValues;
use lavashell_core::scene::tree::SceneUpdate;
use lavashell_core::units::Density;
use lavashell_core::view::{Cursor, InputEvent, Proposal};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Counts the frame requests of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FrameSequence(pub u64);

/// Counts the resizes of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResizeSequence(pub u64);

/// Counts the size proposals of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProposalSequence(pub u64);

#[derive(Serialize, Deserialize)]
pub enum HostMessage {
    /// The first message after the hello.
    Setup {
        /// The effects registered in the renderer of the host.
        #[serde(deserialize_with = "sequence")]
        effects: Vec<EffectId>,
        /// The source filters registered in the renderer of the host.
        #[serde(deserialize_with = "sequence")]
        filters: Vec<SourceFilterId>,
    },
    Preferences(PreferenceValues),
    Resize {
        sequence: ResizeSequence,
        size: NonNegativeSize,
        density: Density,
    },
    Propose {
        sequence: ProposalSequence,
        proposal: Proposal,
    },
    Frame {
        sequence: FrameSequence,
        presentation: Duration,
        deadline: Duration,
    },
    Input {
        /// The sequence of the frame shown when the input arrived.
        shown: FrameSequence,
        input: InputEvent,
    },
    Text(TextCommand),
    Copy,
    Cut,
    Paste(String),
    Accessibility(ActionRequest),
    AccessibilityActive(bool),
    Shown,
    Hidden,
    Throttled,
}

#[derive(Serialize, Deserialize)]
pub enum GuestMessage {
    RequestFrame,
    Scene {
        /// The sequence of the frame request that the update is sent for.
        frame: FrameSequence,
        /// The sequence of the resize that the update is laid out for.
        layout: ResizeSequence,
        update: SceneUpdate,
    },
    Cursor(Cursor),
    FocusVisibleInside(bool),
    Session(Option<TextSession>),
    Copied(String),
    Accessibility(AccessibilityUpdate),
    Size {
        /// The sequence of the proposal that the size is sent for.
        sequence: ProposalSequence,
        size: NonNegativeSize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sendable<T: Send>() {}

    #[test]
    fn decoded_messages_can_move_to_another_thread() {
        sendable::<HostMessage>();
        sendable::<GuestMessage>();
    }
}
