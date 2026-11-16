// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn ids_count_up_from_zero() {
    let mut ids = IdCounter::default();
    assert_eq!(ids.next_id(), Ok(0));
    assert_eq!(ids.next_id(), Ok(1));
}

#[test]
fn the_largest_id_is_the_last_one_handed_out() {
    let mut ids = IdCounter {
        last: Some(u64::MAX - 1),
    };
    assert_eq!(ids.next_id(), Ok(u64::MAX));
    assert_eq!(ids.next_id(), Err(IdsExhausted));
    assert_eq!(ids.next_id(), Err(IdsExhausted));
}
