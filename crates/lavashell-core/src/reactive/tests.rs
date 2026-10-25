// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn dependencies(scope: &Scope, phase: Phase) -> usize {
    (scope.0.dependencies.borrow().iter())
        .filter(|(read, _)| *read == phase)
        .count()
}

#[test]
fn all_dirty_contains_every_phase_and_the_default_none() {
    for phase in [Phase::Composition, Phase::Layout, Phase::Paint] {
        assert!(Dirty::ALL.contains(phase));
        assert!(!Dirty::default().contains(phase));
    }
    assert_eq!(Dirty::ALL.0.count_ones() as usize, Phase::COUNT);
}

#[test]
fn tracks_only_the_read_phase_and_deduplicates() {
    let scope = Scope::default();
    let value = State::new(0);
    assert_eq!(scope.track(Phase::Paint, || value.get() + value.get()), 0);
    assert_eq!(dependencies(&scope, Phase::Paint), 1);
    value.set(1);
    assert!(scope.dirty().contains(Phase::Paint));
    assert!(!scope.dirty().contains(Phase::Composition));
}

#[test]
fn tracking_a_state_without_its_value_follows_its_changes() {
    let scope = Scope::default();
    let value = State::new(String::from("unread"));
    scope.track(Phase::Layout, || value.track());
    assert_eq!(dependencies(&scope, Phase::Layout), 1);
    value.set(String::from("changed"));
    assert!(scope.dirty().contains(Phase::Layout));
}

#[test]
fn peeking_at_a_state_without_cloning_it_follows_nothing() {
    let scope = Scope::default();
    let value = State::new(String::from("unread"));
    assert_eq!(
        scope.track(Phase::Layout, || value.peek_with(String::len)),
        6
    );
    assert_eq!(dependencies(&scope, Phase::Layout), 0);
    value.set(String::from("changed"));
    assert!(!scope.dirty().any());
}

#[test]
fn conditional_dependencies_expire() {
    let scope = Scope::default();
    let a = State::new(1);
    let b = State::new(2);
    assert_eq!(scope.track(Phase::Composition, || a.get()), 1);
    assert_eq!(scope.track(Phase::Composition, || b.get()), 2);
    a.set(3);
    assert!(!scope.dirty().any());
    b.set(4);
    assert!(scope.dirty().any());
}

#[test]
fn nested_scopes_restore_tracking() {
    let outer = Scope::default();
    let inner = Scope::default();
    let a = State::new(1);
    let b = State::new(2);
    let read = outer.track(Phase::Layout, || {
        inner.track(Phase::Paint, || a.get()) + b.get()
    });
    assert_eq!(read, 3);
    a.set(3);
    assert!(!outer.dirty().any());
    assert!(inner.dirty().contains(Phase::Paint));
    b.set(4);
    assert!(outer.dirty().contains(Phase::Layout));
}

#[test]
fn further_tracking_adds_to_the_dependencies_of_the_last_track() {
    let s = Scope::default();
    let (a, b) = (State::new(1), State::new(2));
    assert_eq!(s.track(Phase::Layout, || a.get()), 1);
    assert_eq!(s.track_further(Phase::Layout, || b.get()), 2);
    assert_eq!(dependencies(&s, Phase::Layout), 2);
    a.set(3);
    assert!(s.dirty().contains(Phase::Layout));
    s.track(Phase::Layout, || {});
    b.set(4);
    assert!(!s.dirty().any());
}

#[test]
fn a_derived_value_follows_the_states_it_reads_and_notifies_only_when_it_changes() {
    let (switch, a, b) = (State::new(false), State::new(1), State::new(10));
    let (chosen, first, second) = (switch.clone(), a.clone(), b.clone());
    let derived = Derived::new(move || {
        (if chosen.get() {
            second.get()
        } else {
            first.get()
        }) >= 5
    });
    let s = Scope::default();
    assert!(!s.track(Phase::Composition, || derived.get()));
    a.set(2);
    b.set(1);
    assert!(!s.dirty().any());
    a.set(7);
    assert!(s.dirty().any());
    assert!(s.track(Phase::Composition, || derived.get()));
    switch.set(true);
    assert!(!derived.get());
    assert!(s.dirty().any());
    assert!(!s.track(Phase::Composition, || derived.get()));
    b.set(8);
    assert!(s.dirty().any());
    assert!(a.is_read());
    drop(derived);
    assert!(!a.is_read());
}

#[test]
fn equality_suppresses_work() {
    let s = Scope::default();
    let a = State::new(1);
    assert_eq!(s.track(Phase::Composition, || a.get()), 1);
    a.set_if_changed(1);
    assert!(!s.dirty().any());
}
