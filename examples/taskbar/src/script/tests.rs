// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_script_plays_its_cues_in_order_and_starts_again_after_the_last() {
    let mut script = Script::default();
    let played: Vec<_> = (0..CUES.len() * 2).map(|_| script.next_cue()).collect();
    assert_eq!(played[..CUES.len()], CUES);
    assert_eq!(played[CUES.len()..], CUES);
}
