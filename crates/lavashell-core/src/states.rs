// SPDX-License-Identifier: GPL-3.0-only
use crate::reactive::State;
use std::any::{Any, TypeId};
use std::collections::BTreeMap;

/// States of any type, stored under a key and the type of the state, so states of two types never share an entry.
pub(crate) struct States<K>(BTreeMap<(K, TypeId), Box<dyn Any>>);

impl<K> Default for States<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord> States<K> {
    /// The state of type `T` stored under `key`, or a new one that starts at `init()`.
    pub(crate) fn state<T: 'static>(&mut self, key: K, init: impl FnOnce() -> T) -> State<T> {
        let key = (key, TypeId::of::<T>());
        if let Some(state) = (self.0.get(&key)).and_then(|stored| stored.downcast_ref::<State<T>>())
        {
            return state.clone();
        }
        let state = State::new(init());
        let _replaced_state = self.0.insert(key, Box::new(state.clone()));
        state
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.0.retain(|(key, _), _| keep(key));
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_key_keeps_a_state_for_each_type() {
        let mut states = States::default();
        let number = states.state("open", || 1_u32);
        let flag = states.state("open", || false);
        number.set(2);
        flag.set(true);
        assert_eq!(states.state("open", || 0_u32).get(), 2);
        assert!(states.state("open", || false).get());
        assert_eq!(states.len(), 2);
    }
}
