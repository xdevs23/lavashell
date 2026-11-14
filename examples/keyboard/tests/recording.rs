// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::constant;
use lavashell_example_support::{FrameInterval, Recording};
use lavashell_on_screen_keyboard::Error;
use std::path::Path;

#[test]
fn a_recording_writes_each_sequence_and_each_still() -> Result<(), Error> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("keyboard-recording");
    let _absent_before_the_first_run = std::fs::remove_dir_all(&directory);
    let recording = Recording {
        directory,
        interval: constant!(FrameInterval::per_second(1)),
    };
    lavashell_on_screen_keyboard::recording::record(&recording)?;
    for sequence in ["show", "hide", "preview", "previews", "dock"] {
        let first = recording.directory.join(sequence).join("000.png");
        assert!(first.is_file(), "the sequence {sequence} has a first frame");
    }
    for still in ["reduced-motion-before", "reduced-motion-after"] {
        let path = recording.directory.join(format!("{still}.png"));
        assert!(path.is_file(), "the still {still} is written");
    }
    Ok(())
}
