// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Set { key: u8, value: f64 },
    Step(f64),
    Fence(u8),
}

impl Merge for Item {
    type Key = u8;

    fn effect(&self) -> Effect<u8> {
        match *self {
            Self::Set { key, .. } => Effect::Overwrites(key),
            Self::Step(_) => Effect::BuildsOn,
            Self::Fence(key) => Effect::Fences(key),
        }
    }
}

/// An amount that adds up with the later amounts of its key.
#[derive(Debug, PartialEq)]
struct Sum {
    key: u8,
    amount: f64,
}

impl Merge for Sum {
    type Key = u8;

    fn effect(&self) -> Effect<u8> {
        Effect::Overwrites(self.key)
    }

    fn absorb(&mut self, later: Self) {
        self.amount += later.amount;
    }
}

fn set(key: u8, value: f64) -> Item {
    Item::Set { key, value }
}

fn step(number: f64) -> Item {
    Item::Step(number)
}

/// Whether the taken frame restarts, and its items.
fn take<T: Merge>(choreographer: &mut Choreographer<T>) -> Option<(bool, Vec<T>)> {
    let frame = choreographer.take()?;
    Some((frame.restart, frame.items))
}

fn send<T: Merge>(choreographer: &mut Choreographer<T>, frame: impl IntoIterator<Item = T>) {
    for item in frame {
        choreographer.push(item);
    }
    choreographer.end_frame();
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
    choreographer.end_frame();
    assert_eq!(take(&mut choreographer), Some((false, vec![set(0, 3.0)])));
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
