// SPDX-License-Identifier: GPL-3.0-only
//! Values passed down the view tree by key. A wrapper provides a value for the subtree composed inside it. Code in
//! that subtree reads the value provided by the nearest wrapper, or else the root value of the key.

use crate::platform::PlatformFacts;
use crate::reactive::{Derived, State};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

/// A key for a value handed down the view tree, with a function that returns the value read
/// where neither a wrapper nor the view provides one:
/// ```
/// # use lavashell_core::local::Local;
/// pub static ACCENT: Local<u32> = Local::new(|| 0x3366ff);
/// ```
/// A key declared as a `const` does not compile where it is read or provided:
/// ```compile_fail,E0716
/// # use lavashell_core::{local::Local, ui::Ui};
/// const ACCENT: Local<u32> = Local::new(|| 0x3366ff);
/// fn accent(ui: &Ui) -> u32 {
///     ui.local(&ACCENT)
/// }
/// ```
pub struct Local<T> {
    fallback: Fallback<T>,
    /// Makes `Local` interior mutable. The compiler then does not promote `&KEY` of a `const` key to a
    /// `'static` reference, so a `const` key fails to compile where it is read or provided.
    _not_const: AtomicBool,
}

/// Where the root value of a key comes from until the view provides one.
enum Fallback<T> {
    Declared(fn() -> T),
    Platform(fn(&PlatformFacts) -> T),
}

impl<T> Local<T> {
    /// A key whose root value is returned by `default`. Declare the key as a `static`. Keys are told apart by the
    /// address of the static that declares them, so two keys can have the same value type.
    pub const fn new(default: fn() -> T) -> Self {
        Self {
            fallback: Fallback::Declared(default),
            _not_const: AtomicBool::new(false),
        }
    }

    /// A key whose root value is returned by `read` from the platform facts of the view. The root value
    /// is read again each time the host replaces the facts with `View::provide_platform`, until the
    /// view provides a value for the key.
    pub const fn platform(read: fn(&PlatformFacts) -> T) -> Self {
        Self {
            fallback: Fallback::Platform(read),
            _not_const: AtomicBool::new(false),
        }
    }
}

impl<T: 'static> Local<T> {
    fn id(&'static self) -> KeyId {
        KeyId {
            address: std::ptr::from_ref(self).addr(),
            value: TypeId::of::<T>(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Local<T> {
    /// A function that returns the value read where neither a wrapper nor the view provides one. For a platform
    /// key, the function reads the facts currently in `platform`.
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

/// The address of the static declaring a key, and the type of its values.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KeyId {
    address: usize,
    value: TypeId,
}

/// The root values of a view, one for each key read or provided so far, and the platform facts
/// from which the root values of platform keys are read.
#[derive(Clone)]
pub(crate) struct RootValues(Rc<RefCell<Roots>>);

struct Roots {
    platform: State<PlatformFacts>,
    values: BTreeMap<KeyId, Box<dyn Any>>,
}

/// The root value of one key: the value that the view provides, or else the fallback of the key.
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

    /// The root value of `key` as a derived value. A scope that reads the derived value is notified when the root
    /// value changes.
    pub(crate) fn value<T: Clone + PartialEq + 'static>(
        &self,
        key: &'static Local<T>,
    ) -> Derived<T> {
        self.root(key).value
    }

    /// Sets the root value of `key`. From then on, `value` replaces the fallback of the key.
    pub(crate) fn provide<T: Clone + PartialEq + 'static>(&self, key: &'static Local<T>, value: T) {
        self.root(key).provided.set_if_changed(Some(value));
    }

    pub(crate) fn platform(&self) -> PlatformFacts {
        self.0.borrow().platform.peek()
    }

    /// Replaces the platform facts. The root value of each platform key is read again from the new facts, unless
    /// the view provides a value for the key.
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

/// The values provided around the code being composed, nearest first, and the root values of
/// the view. Only a root value can change, so only a read of a root value subscribes the reader.
#[derive(Clone)]
pub(crate) struct LocalScope {
    provided: Option<Rc<ProvidedValue>>,
    root: RootValues,
}

struct ProvidedValue {
    key: KeyId,
    value: Box<dyn Any>,
    /// The number of values provided around this one, itself included.
    depth: usize,
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
            depth: self.depth() + 1,
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

    fn depth(&self) -> usize {
        self.provided.as_ref().map_or(0, |provided| provided.depth)
    }
}

/// Reads the values for one composition. Each key is resolved once, and again after a wrapper
/// for that key opens or closes.
pub(crate) struct LocalReader {
    scope: LocalScope,
    resolved: RefCell<BTreeMap<KeyId, Box<dyn Any>>>,
    log: Option<ComponentLog>,
}

/// The values that a scoped component reads from outside itself: the values provided around the place where the
/// component was created, and root values. A value provided inside the component is set by the composition of the
/// component itself, so the log skips it.
struct ComponentLog {
    depth: usize,
    reads: RefCell<ReadLog>,
}

impl LocalReader {
    /// A reader for the root composition, which logs nothing.
    pub(crate) fn root(scope: LocalScope) -> Self {
        Self {
            scope,
            resolved: RefCell::default(),
            log: None,
        }
    }

    pub(crate) fn for_component(scope: LocalScope) -> Self {
        let log = ComponentLog {
            depth: scope.depth(),
            reads: RefCell::default(),
        };
        Self {
            log: Some(log),
            ..Self::root(scope)
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
        if let Some(log) = &self.log
            && nearest.is_none_or(|provided| provided.depth <= log.depth)
        {
            log.reads.borrow_mut().record(id, &value);
        }
        let _no_earlier_resolution = self
            .resolved
            .borrow_mut()
            .insert(id, Box::new(value.clone()));
        value
    }

    /// Provides `value` for `key` until `close` is called with the returned scope.
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

    /// The values read by a scoped component from outside itself, or an empty log for the root composition.
    pub(crate) fn take_reads(&self) -> ReadLog {
        self.log
            .as_ref()
            .map(|log| log.reads.take())
            .unwrap_or_default()
    }
}

/// The scope and the resolved value of a key outside a wrapper, restored when the wrapper closes.
pub(crate) struct SavedScope {
    key: KeyId,
    scope: LocalScope,
    resolved: Option<Box<dyn Any>>,
}

/// The values read by a scoped component from the scope in which the component was created, one for each key.
#[derive(Default, PartialEq)]
pub(crate) struct ReadLog(BTreeMap<KeyId, Box<dyn ErasedValue>>);

impl ReadLog {
    /// Records the first value read for `key`.
    fn record<T: Clone + PartialEq + 'static>(&mut self, key: KeyId, value: &T) {
        let _logged_first_read = self.0.entry(key).or_insert_with(|| Box::new(value.clone()));
    }
}

/// A value of any type that compares with another value of the same type.
trait ErasedValue: Any {
    fn equals(&self, other: &dyn Any) -> bool;
}

impl<T: PartialEq + 'static> ErasedValue for T {
    fn equals(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

impl PartialEq for dyn ErasedValue {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}
