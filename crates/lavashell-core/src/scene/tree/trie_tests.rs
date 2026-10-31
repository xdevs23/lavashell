// SPDX-License-Identifier: GPL-3.0-only
use super::trie::Trie;
use crate::test_error::{Required, TestResult};

/// Keys at both ends of the key range, keys that share their low bits and so meet on several levels, and a run of
/// neighbours.
fn keys() -> Vec<u64> {
    let spread = [0, 1, 63, 64, 1 << 12, 1 << 60, u64::MAX - 64, u64::MAX];
    spread.into_iter().chain(100..400).collect()
}

fn filled() -> Trie<u64> {
    let mut trie = Trie::default();
    for key in keys() {
        trie.insert(key, key);
    }
    trie
}

#[test]
fn every_inserted_key_finds_its_value_and_other_keys_find_none() {
    let mut trie = filled();
    for key in keys() {
        assert_eq!(trie.get(key), Some(&key));
    }
    for absent in [2, 65, 1 << 61, u64::MAX - 1] {
        assert_eq!(trie.get(absent), None);
        assert_eq!(trie.get_mut(absent), None);
    }
}

#[test]
fn inserting_a_present_key_replaces_its_value() {
    let mut trie = filled();
    trie.insert(64, 1);
    assert_eq!(trie.get(64), Some(&1));
    assert_eq!(trie.get(0), Some(&0));
}

#[test]
fn removed_keys_are_gone_and_the_others_stay_until_they_are_removed_too() {
    let mut trie = filled();
    let keys = keys();
    let (first, second) = keys.split_at(keys.len() / 2);
    for key in first {
        assert_eq!(trie.remove(*key).as_deref(), Some(key));
        assert_eq!(trie.remove(*key), None);
    }
    for key in first {
        assert_eq!(trie.get(*key), None);
    }
    for key in second {
        assert_eq!(trie.get(*key), Some(key));
    }
    for key in second {
        assert_eq!(trie.remove(*key).as_deref(), Some(key));
    }
    assert!(trie.is_empty());
}

#[test]
fn a_clone_keeps_its_values_and_shares_every_value_that_neither_side_changed() -> TestResult {
    let mut trie = filled();
    let clone = trie.clone();
    let (changed, removed, added) = (150, 64, 2);
    *trie.get_mut(changed).required("value of a present key")? += 1;
    assert_eq!(trie.remove(removed).as_deref(), Some(&removed));
    trie.insert(added, added);
    assert_eq!(trie.get(changed), Some(&151));
    assert_eq!(clone.get(changed), Some(&150));
    assert_eq!(clone.get(removed), Some(&removed));
    assert_eq!(clone.get(added), None);
    let is_shared = |key: &u64| match (trie.get(*key), clone.get(*key)) {
        (Some(mine), Some(theirs)) => std::ptr::eq(mine, theirs),
        _ => false,
    };
    let shared: Vec<u64> = keys().into_iter().filter(is_shared).collect();
    let untouched: Vec<u64> = (keys().into_iter())
        .filter(|key| ![changed, removed].contains(key))
        .collect();
    assert_eq!(shared, untouched);
    Ok(())
}

#[test]
fn removing_every_key_below_a_shared_branch_leaves_the_clone_whole() {
    let mut trie = filled();
    let clone = trie.clone();
    // Only these two keys share their low 60 bits, so removing both empties a chain of branches.
    for key in [0, 1 << 60] {
        assert_eq!(trie.remove(key).as_deref(), Some(&key));
    }
    assert_eq!(trie.get(1 << 12), Some(&(1 << 12)));
    for key in keys() {
        assert_eq!(clone.get(key), Some(&key));
    }
}
