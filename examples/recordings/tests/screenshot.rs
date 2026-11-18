// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_recordings::{Error, write_screenshot};
use lavashell_example_support::platform;
use std::path::{Path, PathBuf};
use std::{fs::File, io::BufReader};

const APP_ID: &str = "test.screenshot";

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Recording(#[from] Error),
    #[error("cannot open the screenshot")]
    Open(#[source] std::io::Error),
    #[error("cannot decode the screenshot")]
    Decode(#[source] png::DecodingError),
}

fn app() -> Result<App, Error> {
    let mut app = App::new(platform());
    app.window(
        Window::new("screenshot", APP_ID).size(10.dp(), 20.dp()),
        |_| {},
    )
    .map_err(lavashell_example_support::Error::Setup)?;
    Ok(app)
}

fn directory() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("screenshot")
}

#[test]
fn a_screenshot_writes_the_last_frame_of_the_window_as_png() -> Result<(), TestError> {
    let _absent_before_the_first_run = std::fs::remove_dir_all(directory());
    let path = directory().join("window.png");
    write_screenshot(app()?, APP_ID, &path)?;
    let file = BufReader::new(File::open(&path).map_err(TestError::Open)?);
    let decoded = png::Decoder::new(file)
        .read_info()
        .map_err(TestError::Decode)?;
    assert_eq!((decoded.info().width, decoded.info().height), (10, 20));
    Ok(())
}

#[test]
fn a_screenshot_of_a_window_that_the_app_does_not_show_fails() -> Result<(), Error> {
    let written = write_screenshot(app()?, "test.absent", &directory().join("absent.png"));
    assert!(matches!(written, Err(Error::NothingShown { .. })));
    Ok(())
}
