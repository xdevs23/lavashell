// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::frame_recorder::record_unlogged;
use lavashell::prelude::{App, Density};
use lavashell_test_platform::{HostScript, PacedLinks, RecordedFrame};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn write_capture(path: impl AsRef<Path>, png: &[u8]) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| Error::CreateDirectory {
            path: parent.to_owned(),
            source,
        })?;
    }
    std::fs::write(path, png).map_err(|source| Error::WriteCapture {
        path: path.to_owned(),
        source,
    })
}

#[must_use]
pub fn capture_path(directory: impl AsRef<Path>, name: &str) -> PathBuf {
    directory.as_ref().join(format!("{name}.png"))
}

pub fn write_screenshot(app: App, app_id: &str, path: &Path) -> Result<(), Error> {
    let script = HostScript::new(Density::ONE).record_paced(PacedLinks::default());
    let frames = record_unlogged(app, script)?;
    match frames.of_window(app_id).last() {
        Some(frame) => write_capture(path, &frame.png),
        None => Err(Error::NothingShown {
            app_id: app_id.to_owned(),
        }),
    }
}

pub fn write_first_frames(frames: Vec<(PathBuf, RecordedFrame)>) -> Result<(), Error> {
    let mut first = BTreeMap::new();
    for (path, frame) in frames {
        let _first_frame_kept = first.entry(path).or_insert(frame.png);
    }
    (first.into_iter()).try_for_each(|(path, png)| write_capture(path, &png))
}
