// SPDX-License-Identifier: GPL-3.0-only

use crate::Store;

#[derive(Default)]
struct Names(Vec<&'static str>);

#[derive(Default)]
struct Counter(u32);

#[test]
fn a_layer_registered_twice_keeps_both_additions() {
    let mut store = Store::default();
    store.layer(|names: &mut Names| names.0.push("first"));
    store.layer(|names: &mut Names| names.0.push("second"));

    let read_once = store.get::<Names>().map(|names| names.0.as_slice());
    let read_again = store.get::<Names>().map(|names| names.0.as_slice());
    assert_eq!(read_once, Some(["first", "second"].as_slice()));
    assert_eq!(read_again, read_once);
}

#[test]
fn a_layer_nobody_registered_is_absent() {
    assert!(Store::default().get::<Names>().is_none());
}

#[test]
fn two_layers_do_not_see_each_other() {
    let mut store = Store::default();
    store.layer(|names: &mut Names| names.0.push("only"));
    store.layer(|counter: &mut Counter| counter.0 += 1);

    assert_eq!(
        store.get::<Names>().map(|names| names.0.as_slice()),
        Some(["only"].as_slice())
    );
    assert_eq!(store.get::<Counter>().map(|counter| counter.0), Some(1));
}
