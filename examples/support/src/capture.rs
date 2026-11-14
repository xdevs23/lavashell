// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
#[cfg(target_os = "linux")]
use crate::frame_log::record_unlogged;
#[cfg(target_os = "linux")]
use lavashell::prelude::{App, Density};
#[cfg(target_os = "linux")]
use lavashell_test_platform::{HostScript, Pace};
use std::path::Path;

/// Writes the PNG file `png` to `path`, and creates the parent directory of `path` first.
///
/// # Errors
/// Returns an error when the directory cannot be created or the file cannot be written.
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

/// Runs `app` on a test platform until it comes to rest, then writes the last frame shown by its window with `app_id`
/// to `path`, as [`write_capture`] does.
///
/// # Errors
/// Returns an error when the app fails, when a component failed in a frame, when the window did not show a frame, or
/// when the file cannot be written.
#[cfg(target_os = "linux")]
pub fn write_screenshot(app: App, app_id: &str, path: &Path) -> Result<(), Error> {
    let script = HostScript::new(Density::ONE).record_paced(Pace::default());
    let frames = record_unlogged(app, script)?;
    match frames.of_window(app_id).last() {
        Some(frame) => write_capture(path, &frame.png),
        None => Err(Error::NothingShown {
            app_id: app_id.to_owned(),
        }),
    }
}
