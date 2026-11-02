// SPDX-License-Identifier: GPL-3.0-only
//! A change sink for tests that records what it receives and applies each change to a scene tree.

use super::tree::{ApplyError, Change, SceneTree};
use super::{ChangeSink, SinkId};
use std::collections::BTreeSet;

/// One call received by a sink, with a change reduced to the kind of change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Received {
    Insert,
    Remove,
    Move,
    SetContent,
    SetSlot,
    End,
    Restart,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum TestSinkError {
    #[error("the test sink fails the call {0:?} on purpose")]
    Planned(Received),
    #[error("a change does not fit the tree in the test sink")]
    Apply(#[source] ApplyError),
}

#[derive(Default)]
pub(crate) struct TestSink {
    id: SinkId,
    pub tree: SceneTree,
    pub received: Vec<Received>,
    /// The positions in `received` at which a call fails once, without effect on `tree`.
    pub failing: BTreeSet<usize>,
}

impl TestSink {
    fn record(&mut self, call: Received) -> Result<(), TestSinkError> {
        let fails = self.failing.remove(&self.received.len());
        self.received.push(call);
        if fails {
            Err(TestSinkError::Planned(call))
        } else {
            Ok(())
        }
    }
}

impl ChangeSink for TestSink {
    type Error = TestSinkError;

    fn id(&self) -> SinkId {
        self.id
    }

    fn change(&mut self, change: Change) -> Result<(), TestSinkError> {
        self.record(match change {
            Change::Insert { .. } => Received::Insert,
            Change::Remove { .. } => Received::Remove,
            Change::Move { .. } => Received::Move,
            Change::SetContent { .. } => Received::SetContent,
            Change::SetSlot { .. } => Received::SetSlot,
        })?;
        (self.tree.show_change(change)).map_err(TestSinkError::Apply)
    }

    fn end_frame(&mut self) -> Result<(), TestSinkError> {
        self.record(Received::End)
    }

    fn restart(&mut self) -> Result<(), TestSinkError> {
        self.record(Received::Restart)?;
        self.tree = SceneTree::default();
        Ok(())
    }
}
