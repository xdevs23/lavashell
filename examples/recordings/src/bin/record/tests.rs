// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use clap::CommandFactory;

fn parse(arguments: &[&str]) -> Result<RecordedExample, clap::Error> {
    RecordedExample::try_parse_from(std::iter::once("record").chain(arguments.iter().copied()))
}

#[test]
fn the_options_of_each_example_build_one_command() {
    RecordedExample::command().debug_assert();
}

#[test]
fn a_recording_takes_a_directory_a_frame_rate_and_a_theme() {
    let keyboard = parse(&[
        "keyboard",
        "--frames",
        "out",
        "--frame-rate",
        "30",
        "--theme",
        "dark",
    ]);
    assert!(matches!(
        keyboard,
        Ok(RecordedExample::Keyboard {
            theme: ThemeChoice::Dark,
            ..
        })
    ));
    assert!(parse(&["keyboard", "--frames", "out"]).is_err());
}

#[test]
fn a_taskbar_recording_takes_both_sides_of_its_size() {
    let width_only = [
        "taskbar",
        "--frames",
        "out",
        "--frame-rate",
        "30",
        "--width",
        "800",
    ];
    assert!(parse(&width_only).is_err());
}

#[test]
fn a_screenshot_takes_a_file() {
    assert!(parse(&["materials"]).is_err());
    assert!(parse(&["music-player"]).is_err());
}
