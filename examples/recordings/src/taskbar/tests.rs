// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::collections::BTreeSet;

#[test]
fn a_recording_shows_every_cue_in_at_least_one_frame() -> Result<(), Error> {
    let interval = constant!(FrameInterval::per_second(10));
    let shown: BTreeSet<_> = (record_cues(interval, None, ThemeChoice::System)?.into_iter())
        .map(|(cue, _)| cue)
        .collect();
    assert_eq!(shown, (0..CUES.len()).collect());
    Ok(())
}
