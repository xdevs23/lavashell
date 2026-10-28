// SPDX-License-Identifier: GPL-3.0-only
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

/// Defines `Phase` and counts its variants from the same list, so `Phase::COUNT` always equals the number of
/// variants.
macro_rules! phases {
    ($($phase:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Phase {
            $($phase),+
        }

        impl Phase {
            const COUNT: usize = [$(Self::$phase),+].len();
        }
    };
}

phases!(Composition, Layout, Paint);

impl Phase {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dirty(u8);

impl Dirty {
    /// The set in which every phase is dirty. More phases than bits in a `u8` make this constant fail to compile.
    pub const ALL: Self = Self((1 << Phase::COUNT) - 1);

    #[must_use]
    pub fn contains(self, phase: Phase) -> bool {
        self.0 & (1 << phase.index()) != 0
    }

    #[must_use]
    pub fn any(self) -> bool {
        self.0 != 0
    }
}

#[derive(Default)]
struct ScopeInner {
    epochs: Cell<[u64; Phase::COUNT]>,
    dirty: Cell<Dirty>,
    dependencies: RefCell<BTreeSet<(Phase, u64)>>,
}

#[derive(Clone, Default)]
pub struct Scope(Rc<ScopeInner>);

#[derive(Clone)]
struct Tracking {
    scope: Weak<ScopeInner>,
    phase: Phase,
    epoch: u64,
}

impl Tracking {
    /// The scope, if it still exists and its latest `track` of this phase made this subscription. A subscription
    /// from an earlier `track` is stale.
    fn live(&self) -> Option<Rc<ScopeInner>> {
        (self.scope.upgrade()).filter(|scope| scope.epochs.get()[self.phase.index()] == self.epoch)
    }
}

#[derive(Clone)]
enum Tracker {
    Scope(Tracking),
    Derivation(Weak<dyn Observer>),
    Untracked,
}

thread_local! { static TRACKERS: RefCell<Vec<Tracker>> = const { RefCell::new(Vec::new()) }; }

/// Pushes a tracker onto the stack of trackers of the thread and removes it again when dropped. `depth` is the
/// height of the stack below the tracker.
struct TrackingFrame {
    depth: usize,
}

impl TrackingFrame {
    fn push(tracker: Tracker) -> Self {
        TRACKERS.with(|trackers| {
            let mut trackers = trackers.borrow_mut();
            let depth = trackers.len();
            trackers.push(tracker);
            Self { depth }
        })
    }
}

impl Drop for TrackingFrame {
    fn drop(&mut self) {
        TRACKERS.with(|trackers| trackers.borrow_mut().truncate(self.depth));
    }
}

trait Observer {
    fn changed(&self);
}

pub(crate) fn untracked<R>(f: impl FnOnce() -> R) -> R {
    let _frame = TrackingFrame::push(Tracker::Untracked);
    f()
}

impl Scope {
    pub fn track<R>(&self, phase: Phase, f: impl FnOnce() -> R) -> R {
        self.forget(phase);
        self.track_further(phase, f)
    }

    /// Drops the dependencies recorded for `phase` and clears the dirty flag of `phase`. A change to a value read
    /// before then marks nothing dirty.
    pub fn forget(&self, phase: Phase) {
        let mut epochs = self.0.epochs.get();
        epochs[phase.index()] += 1;
        self.0.epochs.set(epochs);
        self.0
            .dependencies
            .borrow_mut()
            .retain(|(p, _)| *p != phase);
        self.clean(phase);
    }

    /// Clears the dirty flag of `phase` and leaves the dependencies in place. A later change to a value read before
    /// marks `phase` dirty again.
    pub(crate) fn clean(&self, phase: Phase) {
        self.0
            .dirty
            .set(Dirty(self.0.dirty.get().0 & !(1 << phase.index())));
    }

    /// Runs `f` and adds what it reads to the dependencies of `phase` recorded by the last
    /// `track` of that phase, instead of replacing them.
    pub fn track_further<R>(&self, phase: Phase, f: impl FnOnce() -> R) -> R {
        let epoch = self.0.epochs.get()[phase.index()];
        let _frame = TrackingFrame::push(Tracker::Scope(Tracking {
            scope: Rc::downgrade(&self.0),
            phase,
            epoch,
        }));
        f()
    }

    #[must_use]
    pub fn dirty(&self) -> Dirty {
        self.0.dirty.get()
    }

    pub fn invalidate(&self, phase: Phase) {
        self.0
            .dirty
            .set(Dirty(self.0.dirty.get().0 | (1 << phase.index())));
    }
}

struct SignalInner<T> {
    id: u64,
    value: RefCell<T>,
    subscribers: RefCell<Subscribers>,
    observers: RefCell<Vec<Weak<dyn Observer>>>,
}

/// The scopes that read a state, with one entry for each scope and phase. A read then costs one map lookup,
/// however many scopes read the state.
#[derive(Default)]
struct Subscribers {
    entries: BTreeMap<(usize, Phase), Tracking>,
    /// The number of entries after dropped scopes were last removed.
    live: usize,
}

impl Subscribers {
    fn insert(&mut self, tracking: Tracking) {
        let key = (tracking.scope.as_ptr().addr(), tracking.phase);
        let _superseded_tracking_of_the_scope = self.entries.insert(key, tracking);
        // Removing dropped scopes only once the entries reach twice the live count makes an insert cheap on average.
        if self.entries.len() > 2 * self.live.max(1) {
            self.entries.retain(|_, s| s.scope.strong_count() > 0);
            self.live = self.entries.len();
        }
    }
}

pub struct State<T>(Rc<SignalInner<T>>);

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> State<T> {
    pub fn new(value: T) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(Rc::new(SignalInner {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            value: RefCell::new(value),
            subscribers: RefCell::default(),
            observers: RefCell::new(Vec::new()),
        }))
    }

    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.subscribe();
        read(&self.0.value.borrow())
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }

    /// Makes the scope or the derivation that runs this call depend on the state, without
    /// reading its value.
    pub fn track(&self) {
        self.subscribe();
    }

    /// A `Selector` that derives a value from this state with `select`. Its readers are notified only when the
    /// selected value changes. Keep the selector outside composition, or remember it with `Ui::state`.
    pub fn select<U: Clone + PartialEq + 'static>(
        &self,
        select: impl Fn(&T) -> U + 'static,
    ) -> Selector<T, U>
    where
        T: 'static,
    {
        let source = self.clone();
        Selector {
            source: self.clone(),
            selected: Derived::new(move || source.with(&select)),
        }
    }

    pub(crate) fn is_read(&self) -> bool {
        (self.0.subscribers.borrow().entries.values()).any(|tracking| tracking.live().is_some())
            || (self.0.observers.borrow().iter()).any(|observer| observer.strong_count() > 0)
    }

    /// Returns the value without subscribing the current scope. Use this only where the reader already depends on
    /// a `Selector` of this state.
    #[must_use]
    pub fn peek(&self) -> T
    where
        T: Clone,
    {
        self.0.value.borrow().clone()
    }

    /// Passes the value to `read` as `with` does, without subscribing the current scope. Use this only where the
    /// reader already depends on a `Selector` of this state.
    pub fn peek_with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.0.value.borrow())
    }

    pub fn set(&self, value: T) {
        *self.0.value.borrow_mut() = value;
        self.notify();
    }

    pub fn update<R>(&self, update: impl FnOnce(&mut T) -> R) -> R {
        let result = update(&mut self.0.value.borrow_mut());
        self.notify();
        result
    }

    /// An event callback that applies `update` to the state. The callback contains one clone of the handle, made
    /// when the callback is created.
    pub fn action(&self, mut update: impl FnMut(&mut T) + 'static) -> impl FnMut() + 'static
    where
        T: 'static,
    {
        let state = self.clone();
        move || state.update(&mut update)
    }

    pub fn handler<A>(&self, mut update: impl FnMut(&mut T, A) + 'static) -> impl FnMut(A) + 'static
    where
        T: 'static,
    {
        let state = self.clone();
        move |value| state.update(|current| update(current, value))
    }

    pub fn set_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        let changed = {
            let mut old = self.0.value.borrow_mut();
            if *old == value {
                false
            } else {
                *old = value;
                true
            }
        };
        if changed {
            self.notify();
        }
    }

    fn subscribe(&self) {
        let tracker = TRACKERS.with(|t| t.borrow().last().cloned());
        match tracker {
            None | Some(Tracker::Untracked) => {}
            Some(Tracker::Scope(tracking)) => {
                if let Some(scope) = tracking.scope.upgrade() {
                    let _already_followed_in_this_phase =
                        (scope.dependencies.borrow_mut()).insert((tracking.phase, self.0.id));
                }
                self.0.subscribers.borrow_mut().insert(tracking);
            }
            Some(Tracker::Derivation(derivation)) => {
                let mut observers = self.0.observers.borrow_mut();
                if !observers
                    .iter()
                    .any(|observer| observer.ptr_eq(&derivation))
                {
                    observers.push(derivation);
                }
            }
        }
    }

    fn notify(&self) {
        {
            let mut subscribers = self.0.subscribers.borrow_mut();
            subscribers.entries.retain(|_, s| {
                let Some(scope) = s.live() else {
                    return false;
                };
                scope
                    .dirty
                    .set(Dirty(scope.dirty.get().0 | (1 << s.phase.index())));
                true
            });
            subscribers.live = subscribers.entries.len();
        }
        let observers = {
            let mut observers = self.0.observers.borrow_mut();
            observers.retain(|observer| observer.strong_count() > 0);
            observers
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>()
        };
        for observer in observers {
            observer.changed();
        }
    }
}

struct Derivation<U> {
    tracker: Weak<dyn Observer>,
    derive: Box<dyn Fn() -> U>,
    value: State<U>,
}

impl<U: PartialEq> Observer for Derivation<U> {
    fn changed(&self) {
        self.value
            .set_if_changed(tracked(&self.tracker, self.derive.as_ref()));
    }
}

fn tracked<U>(tracker: &Weak<dyn Observer>, derive: &dyn Fn() -> U) -> U {
    let _frame = TrackingFrame::push(Tracker::Derivation(tracker.clone()));
    derive()
}

/// A value computed by a function from the states that it reads, and computed again when one of them changes. Keep
/// the handle outside composition, or remember it with `Ui::state`.
pub struct Derived<U>(Rc<Derivation<U>>);

impl<U> Clone for Derived<U> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<U: Clone + PartialEq + 'static> Derived<U> {
    pub fn new(derive: impl Fn() -> U + 'static) -> Self {
        Self(Rc::new_cyclic(|derivation: &Weak<Derivation<U>>| {
            let tracker: Weak<dyn Observer> = derivation.clone();
            Derivation {
                value: State::new(tracked(&tracker, &derive)),
                tracker,
                derive: Box::new(derive),
            }
        }))
    }
}

impl<U> Derived<U> {
    #[must_use]
    pub fn get(&self) -> U
    where
        U: Clone,
    {
        self.0.value.get()
    }

    pub fn with<R>(&self, read: impl FnOnce(&U) -> R) -> R {
        self.0.value.with(read)
    }
}

#[derive(Clone)]
pub struct Selector<T, U> {
    source: State<T>,
    selected: Derived<U>,
}

impl<T, U: Clone> Selector<T, U> {
    #[must_use]
    pub fn get(&self) -> U {
        self.selected.get()
    }

    pub fn with<R>(&self, read: impl FnOnce(&U) -> R) -> R {
        self.selected.with(read)
    }

    #[must_use]
    pub fn source(&self) -> &State<T> {
        &self.source
    }
}

#[cfg(test)]
mod tests;
