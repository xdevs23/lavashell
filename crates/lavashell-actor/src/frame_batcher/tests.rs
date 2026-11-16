// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_items::{Item, Sum, set, step};

fn take<T: Merge>(batcher: &mut FrameBatcher<T, ()>) -> Option<(bool, Vec<T>)> {
    let frame = batcher.take()?;
    Some((frame.restart, frame.items))
}

fn parts<T, E>(frame: Frame<T, E>) -> (bool, Vec<T>, Option<E>) {
    (frame.restart, frame.items, frame.end)
}

fn send<T: Merge>(batcher: &mut FrameBatcher<T, ()>, frame: impl IntoIterator<Item = T>) {
    for item in frame {
        batcher.push(item);
    }
    batcher.end_frame(None);
}

#[test]
fn a_value_set_nine_times_while_the_consumer_is_busy_arrives_as_the_last_value_of_each_take() {
    let mut batcher = FrameBatcher::default();
    let mut taken = Vec::new();
    for round in 1..=9_u8 {
        send(&mut batcher, [set(0, f64::from(round))]);
        if [5, 8, 9].contains(&round) {
            taken.extend(take(&mut batcher));
        }
    }
    let expected = [5.0, 8.0, 9.0].map(|value| (false, vec![set(0, value)]));
    assert_eq!(taken, expected);
}

#[test]
fn an_overwrite_takes_the_place_of_the_first_write_and_every_step_keeps_its_order() {
    let mut batcher = FrameBatcher::default();
    send(
        &mut batcher,
        [step(1.0), set(0, 1.0), step(2.0), set(1, 1.0)],
    );
    send(&mut batcher, [set(1, 2.0), step(3.0), set(0, 2.0)]);
    let merged = [step(1.0), set(0, 2.0), step(2.0), set(1, 2.0), step(3.0)];
    assert_eq!(take(&mut batcher), Some((false, merged.to_vec())));
    assert_eq!(take(&mut batcher), None);
}

#[test]
fn a_fence_keeps_a_later_write_of_its_key_after_it() {
    let mut batcher = FrameBatcher::default();
    send(&mut batcher, [set(0, 1.0)]);
    send(&mut batcher, [Item::Fence(0)]);
    send(&mut batcher, [set(0, 2.0)]);
    send(&mut batcher, [set(0, 3.0)]);
    let merged = vec![set(0, 1.0), Item::Fence(0), set(0, 3.0)];
    assert_eq!(take(&mut batcher), Some((false, merged)));
}

#[test]
fn the_open_frame_is_handed_out_only_once_it_ends() {
    let mut batcher = FrameBatcher::default();
    assert_eq!(take(&mut batcher), None);
    send(&mut batcher, [set(0, 1.0)]);
    batcher.push(set(0, 2.0));
    batcher.push(set(0, 3.0));
    assert_eq!(take(&mut batcher), Some((false, vec![set(0, 1.0)])));
    assert_eq!(take(&mut batcher), None);
    batcher.end_frame(None);
    assert_eq!(take(&mut batcher), Some((false, vec![set(0, 3.0)])));
}

#[test]
fn an_end_value_replaces_the_waiting_end_value_and_a_frame_without_one_keeps_it() {
    let mut batcher = FrameBatcher::<Item, u64>::default();
    batcher.end_frame(Some(1));
    batcher.end_frame(Some(2));
    batcher.end_frame(None);
    assert_eq!(batcher.take().map(|frame| frame.end), Some(Some(2)));
}

#[test]
fn a_restart_keeps_the_end_value_of_the_dropped_frames_for_the_next_take() {
    let mut batcher = FrameBatcher::default();
    batcher.push(step(1.0));
    batcher.end_frame(Some(1));
    batcher.restart();
    let ending = batcher.take().map(parts);
    assert_eq!(ending, Some((false, Vec::new(), Some(1))));
    batcher.push(step(2.0));
    batcher.end_frame(Some(2));
    let restarted = batcher.take().map(parts);
    assert_eq!(restarted, Some((true, vec![step(2.0)], Some(2))));
}

#[test]
fn only_the_waiting_frames_count_their_items_that_build_on_earlier_items() {
    let mut batcher = FrameBatcher::default();
    send(&mut batcher, [step(1.0), set(0, 1.0), Item::Fence(0)]);
    batcher.push(step(2.0));
    assert_eq!(batcher.waiting_appended_items(), 2);
    assert!(take(&mut batcher).is_some());
    assert_eq!(batcher.waiting_appended_items(), 0);
}

#[test]
fn a_restart_drops_the_waiting_and_the_open_frames_and_marks_only_the_next_take() {
    let mut batcher = FrameBatcher::default();
    send(&mut batcher, [step(1.0)]);
    batcher.push(step(2.0));
    batcher.restart();
    assert_eq!(take(&mut batcher), None);
    send(&mut batcher, [step(3.0)]);
    assert_eq!(take(&mut batcher), Some((true, vec![step(3.0)])));
    send(&mut batcher, [step(4.0)]);
    assert_eq!(take(&mut batcher), Some((false, vec![step(4.0)])));
}

#[test]
fn an_item_with_its_own_absorb_folds_every_later_write_of_its_key_into_the_first() {
    let mut batcher = FrameBatcher::default();
    let sum = |amount| Sum { key: 0, amount };
    send(&mut batcher, [sum(1.0), sum(2.0)]);
    send(&mut batcher, [sum(4.0)]);
    assert_eq!(take(&mut batcher), Some((false, vec![sum(7.0)])));
}
