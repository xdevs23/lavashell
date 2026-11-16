// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn a_unit_too_large_for_a_u8_falls_back_to_one_unit_per_scalar() {
    let cluster = "e\u{301}".repeat(100);
    let run = AccessibleTextRun {
        text: cluster.clone(),
        selectable_lengths: vec![200],
        ..Default::default()
    };
    let expected = [
        Unit {
            scalars: 1,
            bytes: 1,
        },
        Unit {
            scalars: 1,
            bytes: 2,
        },
    ]
    .repeat(100);
    assert_eq!(units(&run), expected);
    let fitting = AccessibleTextRun {
        text: "e\u{301}".into(),
        selectable_lengths: vec![2],
        ..Default::default()
    };
    assert_eq!(
        units(&fitting),
        [Unit {
            scalars: 2,
            bytes: 3
        }]
    );
}

#[test]
fn word_starts_are_indices_of_units_within_each_chunk() {
    let text = "ab cd".repeat(60);
    let run = AccessibleTextRun {
        text: text.clone(),
        selectable_lengths: vec![1; 300],
        word_starts: Some((0..300).step_by(5).collect()),
        ..Default::default()
    };
    let runs = split(&run, None);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].units.len(), 255);
    assert_eq!(runs[1].start, 255);
    assert_eq!(runs[0].word_starts.len(), 51);
    assert_eq!(runs[1].word_starts, [0, 5, 10, 15, 20, 25, 30, 35, 40]);
}
