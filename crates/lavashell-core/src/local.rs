// SPDX-License-Identifier: GPL-3.0-only

use crate::platform::PlatformFacts;
use crate::reactive::{Derived, State};
use crate::unique_id::UniqueId;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

pub struct Local<T> {
    fallback: Fallback<T>,
    id: OnceLock<UniqueId>,
}

enum Fallback<T> {
    Declared(fn() -> T),
    Platform(fn(&PlatformFacts) -> T),
}

impl<T> Local<T> {
    pub const fn new(default: fn() -> T) -> Self {
        Self {
            fallback: Fallback::Declared(default),
            id: OnceLock::new(),
        }
    }

    pub const fn platform(read: fn(&PlatformFacts) -> T) -> Self {
        Self {
            fallback: Fallback::Platform(read),
            id: OnceLock::new(),
        }
    }
}

impl<T: 'static> Local<T> {
    fn id(&'static self) -> KeyId {
        KeyId {
            id: *self.id.get_or_init(UniqueId::next),
            value: TypeId::of::<T>(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Local<T> {
    fn fallback_following(&'static self, platform: &State<PlatformFacts>) -> Box<dyn Fn() -> T> {
        match self.fallback {
            Fallback::Declared(default) => Box::new(default),
            Fallback::Platform(read) => {
                let selected = platform.select(read);
                Box::new(move || selected.get())
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KeyId {
    id: UniqueId,
    value: TypeId,
}

#[derive(Clone)]
pub(crate) struct RootValues(Rc<RefCell<Roots>>);

struct Roots {
    platform: State<PlatformFacts>,
    values: BTreeMap<KeyId, Box<dyn Any>>,
}

#[derive(Clone)]
struct RootValue<T> {
    provided: State<Option<T>>,
    value: Derived<T>,
}

impl RootValues {
    pub(crate) fn new(platform: PlatformFacts) -> Self {
        Self(Rc::new(RefCell::new(Roots {
            platform: State::new(platform),
            values: BTreeMap::new(),
        })))
    }

    pub(crate) fn value<T: Clone + PartialEq + 'static>(
        &self,
        key: &'static Local<T>,
    ) -> Derived<T> {
        self.root(key).value
    }

    pub(crate) fn provide<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>, value: T) {
        self.root(key).provided.set_if_changed(Some(value));
    }

    pub(crate) fn platform(&self) -> PlatformFacts {
        self.0.borrow().platform.peek()
    }

    pub(crate) fn set_platform(&self, platform: PlatformFacts) {
        let state = self.0.borrow().platform.clone();
        state.set_if_changed(platform);
    }

    fn root<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>) -> RootValue<T> {
        let mut roots = self.0.borrow_mut();
        let Roots { platform, values } = &mut *roots;
        let stored = values.get(&key.id());
        if let Some(root) = stored.and_then(|stored| stored.downcast_ref::<RootValue<T>>()) {
            return root.clone();
        }
        let provided = State::new(None);
        let fallback = key.fallback_following(platform);
        let read = provided.clone();
        let root = RootValue {
            provided,
            value: Derived::new(move || read.get().unwrap_or_else(&fallback)),
        };
        let _none_for_a_key_without_a_root_value = values.insert(key.id(), Box::new(root.clone()));
        root
    }
}

#[derive(Clone)]
pub(crate) struct LocalScope {
    provided: Option<Rc<ProvidedValue>>,
    root: RootValues,
}

struct ProvidedValue {
    key: KeyId,
    value: Box<dyn Any>,
    outer: Option<Rc<ProvidedValue>>,
}

impl LocalScope {
    pub(crate) fn root(root: RootValues) -> Self {
        Self {
            provided: None,
            root,
        }
    }

    fn with<T: 'static>(&self, key: KeyId, value: T) -> Self {
        let provided = ProvidedValue {
            key,
            value: Box::new(value),
            outer: self.provided.clone(),
        };
        Self {
            provided: Some(Rc::new(provided)),
            root: self.root.clone(),
        }
    }

    fn nearest(&self, key: KeyId) -> Option<&ProvidedValue> {
        std::iter::successors(self.provided.as_deref(), |provided| {
            provided.outer.as_deref()
        })
        .find(|provided| provided.key == key)
    }
}

pub(crate) struct LocalReader {
    scope: LocalScope,
    resolved: RefCell<BTreeMap<KeyId, Box<dyn Any>>>,
}

impl LocalReader {
    pub(crate) fn new(scope: LocalScope) -> Self {
        Self {
            scope,
            resolved: RefCell::default(),
        }
    }

    pub(crate) fn scope(&self) -> &LocalScope {
        &self.scope
    }

    pub(crate) fn read<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>) -> T {
        let id = key.id();
        if let Some(value) = self
            .resolved
            .borrow()
            .get(&id)
            .and_then(|value| value.downcast_ref::<T>())
        {
            return value.clone();
        }
        let nearest = self.scope.nearest(id);
        let provided = nearest.and_then(|provided| provided.value.downcast_ref::<T>());
        let value = provided.map_or_else(|| self.scope.root.value(key).get(), T::clone);
        let _no_earlier_resolution = self
            .resolved
            .borrow_mut()
            .insert(id, Box::new(value.clone()));
        value
    }

    pub(crate) fn open<T: 'static>(&mut self, key: &'static Local<T>, value: T) -> SavedScope {
        let key = key.id();
        let scope = self.scope.with(key, value);
        SavedScope {
            key,
            scope: std::mem::replace(&mut self.scope, scope),
            resolved: self.resolved.get_mut().remove(&key),
        }
    }

    pub(crate) fn close(&mut self, saved: SavedScope) {
        self.scope = saved.scope;
        let resolved = self.resolved.get_mut();
        let _resolution_inside_the_wrapper = match saved.resolved {
            Some(value) => resolved.insert(saved.key, value),
            None => resolved.remove(&saved.key),
        };
    }
}

pub(crate) struct SavedScope {
    key: KeyId,
    scope: LocalScope,
    resolved: Option<Box<dyn Any>>,
}
