// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell_example_recordings::{Error, write_screenshot};
use lavashell_example_support::{ThemeChoice, default_app};
use lavashell_materials_example::{
    APP_ID, app,
    model::{Demo, Run},
};
use std::path::Path;

// https://www.w3.org/TR/png-3/#5PNG-file-signature
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[test]
fn a_screenshot_settles_without_a_second_frame_at_the_same_virtual_time() -> Result<(), Error> {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("materials-screenshot.png");
    let demo = Demo::new(Run::Screenshot, ThemeChoice::System)?;
    write_screenshot(app(default_app()?, &demo, None, false)?, APP_ID, &path)?;
    assert!(std::fs::read(&path).is_ok_and(|png| png.starts_with(PNG_SIGNATURE)));
    Ok(())
}
