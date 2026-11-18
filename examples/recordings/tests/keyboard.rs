// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::constant;
use lavashell_example_recordings::{Error, FrameInterval, Recording};
use lavashell_example_support::ThemeChoice;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Recording(#[from] Error),
    #[error("cannot list the written recording")]
    List(#[source] std::io::Error),
}

#[test]
fn a_recording_writes_numbered_sequences_and_stills() -> Result<(), TestError> {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("keyboard-recording");
    let _absent_before_the_first_run = std::fs::remove_dir_all(&directory);
    let recording = Recording {
        directory,
        interval: constant!(FrameInterval::per_second(1)),
    };
    lavashell_example_recordings::keyboard::record(&recording, ThemeChoice::Dark)?;
    let listed = std::fs::read_dir(&recording.directory).map_err(TestError::List)?;
    let written: Vec<PathBuf> = (listed.map(|entry| entry.map(|entry| entry.path())))
        .collect::<Result<_, _>>()
        .map_err(TestError::List)?;
    let (sequences, stills): (Vec<_>, Vec<_>) = written.into_iter().partition(|path| path.is_dir());
    assert!(!sequences.is_empty() && !stills.is_empty());
    for sequence in sequences {
        assert!(
            sequence.join("000.png").is_file(),
            "{sequence:?} starts at 000.png"
        );
    }
    for still in stills {
        assert!(
            still
                .extension()
                .is_some_and(|extension| extension == "png"),
            "{still:?}"
        );
    }
    Ok(())
}
