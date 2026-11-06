// SPDX-License-Identifier: GPL-3.0-only
use accesskit::NodeId as NativeId;
use lavashell_core::prelude::NodeId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Node(NodeId),
    TextRun(NodeId, usize),
}

/// Maps Lavashell node IDs, and the text runs of each node, to AccessKit node IDs. A Lavashell node ID can use all 64
/// bits, so no range of values is free for the text runs, which exist only in the AccessKit tree. Every key gets a new
/// AccessKit ID from a counter instead.
#[derive(Default)]
pub(super) struct Ids {
    next: u64,
    native: BTreeMap<Key, NativeId>,
    owners: BTreeMap<NativeId, (NodeId, Option<usize>)>,
}

impl Ids {
    fn intern(&mut self, key: Key) -> NativeId {
        *self.native.entry(key).or_insert_with(|| {
            self.next += 1;
            let id = NativeId(self.next);
            let owner = match key {
                Key::Node(owner) => (owner, None),
                Key::TextRun(owner, run) => (owner, Some(run)),
            };
            let previous = self.owners.insert(id, owner);
            debug_assert!(previous.is_none(), "a new native id has no owner yet");
            id
        })
    }

    pub(super) fn node(&mut self, id: NodeId) -> NativeId {
        self.intern(Key::Node(id))
    }

    pub(super) fn run(&mut self, owner: NodeId, run: usize) -> NativeId {
        self.intern(Key::TextRun(owner, run))
    }

    pub(super) fn owner(&self, id: NativeId) -> Option<(NodeId, Option<usize>)> {
        self.owners.get(&id).copied()
    }

    pub(super) fn retain(&mut self, live: &BTreeSet<NativeId>) {
        self.native.retain(|_, id| live.contains(id));
        self.owners.retain(|id, _| live.contains(id));
    }
}
