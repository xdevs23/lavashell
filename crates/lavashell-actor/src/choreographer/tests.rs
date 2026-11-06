// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_items::{Item, Sum, set, step};

/// Whether the taken frame restarts, and its items.
fn take<T: Merge>(choreographer: &mut Choreographer<T, ()>) -> Option<(bool, Vec<T>)> {
    let frame = choreographer.take()?;
    Some((frame.restart, frame.items))
}

/// Whether `frame` restarts, its items and its end value.
fn parts<T, E>(frame: Frame<T, E>) -> (bool, Vec<T>, Option<E>) {
    (frame.restart, frame.items, frame.end)
}

fn send<T: Merge>(choreographer: &mut Choreographer<T, ()>, frame: impl IntoIterator<Item = T>) {
    for item in frame {
        choreographer.push(item);
    }
    choreographer.end_frame(None);
}

#[test]
fn a_value_set_nine_times_while_the_consumer_is_busy_arrives_as_the_last_value_of_each_take() {
    let mut choreographer = Choreographer::default();
    let mut taken = Vec::new();
    for round in 1..=9_u8 {
        send(&mut choreographer, [set(0, f64::from(round))]);
        if [5, 8, 9].contains(&round) {
            taken.extend(take(&mut choreographer));
        }
    }
    let expected = [5.0, 8.0, 9.0].map(|value| (false, vec![set(0, value)]));
    assert_eq!(taken, expected);
}

#[test]
fn an_overwrite_takes_the_place_of_the_first_write_and_every_step_keeps_its_order() {
    let mut choreographer = Choreographer::default();
    send(
        &mut choreographer,
        [step(1.0), set(0, 1.0), step(2.0), set(1, 1.0)],
    );
    send(&mut choreographer, [set(1, 2.0), step(3.0), set(0, 2.0)]);
    let merged = [step(1.0), set(0, 2.0), step(2.0), set(1, 2.0), step(3.0)];
    assert_eq!(take(&mut choreographer), Some((false, merged.to_vec())));
    assert_eq!(take(&mut choreographer), None);
}

#[test]
fn a_fence_keeps_a_later_write_of_its_key_after_it() {
    let mut choreographer = Choreographer::default();
    send(&mut choreographer, [set(0, 1.0)]);
    send(&mut choreographer, [Item::Fence(0)]);
    send(&mut choreographer, [set(0, 2.0)]);
    send(&mut choreographer, [set(0, 3.0)]);
    let merged = vec![set(0, 1.0), Item::Fence(0), set(0, 3.0)];
    assert_eq!(take(&mut choreographer), Some((false, merged)));
}

#[test]
fn the_open_frame_is_handed_out_only_once_it_ends() {
    let mut choreographer = Choreographer::default();
    assert_eq!(take(&mut choreographer), None);
    send(&mut choreographer, [set(0, 1.0)]);
    choreographer.push(set(0, 2.0));
    choreographer.push(set(0, 3.0));
    assert_eq!(take(&mut choreographer), Some((false, vec![set(0, 1.0)])));
    assert_eq!(take(&mut choreographer), None);
    choreographer.end_frame(None);
    assert_eq!(take(&mut choreographer), Some((false, vec![set(0, 3.0)])));
}

#[test]
fn an_end_value_replaces_the_waiting_end_value_and_a_frame_without_one_keeps_it() {
    let mut choreographer = Choreographer::<Item, u64>::default();
    choreographer.end_frame(Some(1));
    choreographer.end_frame(Some(2));
    choreographer.end_frame(None);
    assert_eq!(choreographer.take().map(|frame| frame.end), Some(Some(2)));
}

#[test]
fn a_restart_keeps_the_end_value_of_the_dropped_frames_for_the_next_take() {
    let mut choreographer = Choreographer::default();
    choreographer.push(step(1.0));
    choreographer.end_frame(Some(1));
    choreographer.restart();
    let ending = choreographer.take().map(parts);
    assert_eq!(ending, Some((false, Vec::new(), Some(1))));
    choreographer.push(step(2.0));
    choreographer.end_frame(Some(2));
    let restarted = choreographer.take().map(parts);
    assert_eq!(restarted, Some((true, vec![step(2.0)], Some(2))));
}

#[test]
fn only_the_waiting_frames_count_their_items_that_build_on_earlier_items() {
    let mut choreographer = Choreographer::default();
    send(&mut choreographer, [step(1.0), set(0, 1.0), Item::Fence(0)]);
    choreographer.push(step(2.0));
    assert_eq!(choreographer.waiting_builds_on(), 2);
    assert!(take(&mut choreographer).is_some());
    assert_eq!(choreographer.waiting_builds_on(), 0);
}

#[test]
fn a_restart_drops_the_waiting_and_the_open_frames_and_marks_only_the_next_take() {
    let mut choreographer = Choreographer::default();
    send(&mut choreographer, [step(1.0)]);
    choreographer.push(step(2.0));
    choreographer.restart();
    assert_eq!(take(&mut choreographer), None);
    send(&mut choreographer, [step(3.0)]);
    assert_eq!(take(&mut choreographer), Some((true, vec![step(3.0)])));
    send(&mut choreographer, [step(4.0)]);
    assert_eq!(take(&mut choreographer), Some((false, vec![step(4.0)])));
}

#[test]
fn an_item_with_its_own_absorb_folds_every_later_write_of_its_key_into_the_first() {
    let mut choreographer = Choreographer::default();
    let sum = |amount| Sum { key: 0, amount };
    send(&mut choreographer, [sum(1.0), sum(2.0)]);
    send(&mut choreographer, [sum(4.0)]);
    assert_eq!(take(&mut choreographer), Some((false, vec![sum(7.0)])));
}
