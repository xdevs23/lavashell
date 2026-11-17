// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::cell::Cell;
use std::rc::Rc;

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn a_run_ends_once_nothing_is_open() {
    let mut end = RunEnd::default();
    assert!(!end.finished(SECOND, true));
    assert!(end.finished(SECOND, false));
    assert_eq!(end.exit_in(SECOND), None);
}

#[test]
fn a_run_ends_at_the_time_set_by_exit_after() {
    let mut end = RunEnd::default();
    end.exit_after(2 * SECOND);
    assert_eq!(end.exit_in(SECOND), Some(SECOND));
    assert!(!end.finished(SECOND, true));
    assert!(end.finished(2 * SECOND, true));
    assert_eq!(end.exit_in(3 * SECOND), Some(Duration::ZERO));
}

#[test]
fn the_exit_predicate_runs_only_while_nothing_else_ends_the_run() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let mut end = RunEnd::default();
    end.exit_when(move || {
        counted.set(counted.get() + 1);
        counted.get() == 2
    });
    assert!(end.finished(SECOND, false));
    assert_eq!(calls.get(), 0);
    assert!(!end.finished(SECOND, true));
    assert!(end.finished(SECOND, true));
    assert_eq!(calls.get(), 2);
}

#[test]
fn each_exit_predicate_ends_the_run() {
    let flags = [Rc::new(Cell::new(false)), Rc::new(Cell::new(false))];
    let mut end = RunEnd::default();
    for flag in &flags {
        let flag = Rc::clone(flag);
        end.exit_when(move || flag.get());
    }
    assert!(!end.finished(SECOND, true));
    flags[1].set(true);
    assert!(end.finished(SECOND, true));
    flags[1].set(false);
    flags[0].set(true);
    assert!(end.finished(SECOND, true));
}

#[test]
fn an_ended_run_ends_while_something_is_open_without_the_exit_predicate() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let mut end = RunEnd::default();
    end.exit_when(move || {
        counted.set(counted.get() + 1);
        false
    });
    end.end();
    assert!(end.finished(SECOND, true));
    assert_eq!(calls.get(), 0);
}
