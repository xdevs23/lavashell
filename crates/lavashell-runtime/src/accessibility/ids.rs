// SPDX-License-Identifier: GPL-3.0-only
use accesskit::NodeId as NativeId;
use lavashell_core::id_counter::{IdCounter, IdsExhausted};
use lavashell_core::prelude::NodeId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Node(NodeId),
    TextRun(NodeId, usize),
}

#[derive(Default)]
pub(super) struct Ids {
    counter: IdCounter,
    native: BTreeMap<Key, NativeId>,
    owners: BTreeMap<NativeId, (NodeId, Option<usize>)>,
}

impl Ids {
    fn intern(&mut self, key: Key) -> Result<NativeId, IdsExhausted> {
        if let Some(&id) = self.native.get(&key) {
            return Ok(id);
        }
        let id = NativeId(self.counter.next_id()?);
        let owner = match key {
            Key::Node(owner) => (owner, None),
            Key::TextRun(owner, run) => (owner, Some(run)),
        };
        let _new_id_had_no_owner = self.owners.insert(id, owner);
        let _key_had_no_id = self.native.insert(key, id);
        Ok(id)
    }

    pub(super) fn node(&mut self, id: NodeId) -> Result<NativeId, IdsExhausted> {
        self.intern(Key::Node(id))
    }

    pub(super) fn nodes<'a>(
        &mut self,
        ids: impl IntoIterator<Item = &'a NodeId>,
    ) -> Result<Vec<NativeId>, IdsExhausted> {
        ids.into_iter().map(|id| self.node(*id)).collect()
    }

    pub(super) fn run(&mut self, owner: NodeId, run: usize) -> Result<NativeId, IdsExhausted> {
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
