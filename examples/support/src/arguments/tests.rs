// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use clap::{CommandFactory, Parser};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the arguments are not accepted")]
    Parse(#[from] clap::Error),
    #[error("the arguments give no recording")]
    NoRecording,
}

#[derive(Parser)]
struct Options {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    size: SizeArguments,
    #[command(flatten)]
    recording: RecordingArguments,
}

fn parse(arguments: &[&str]) -> Result<Options, clap::Error> {
    Options::try_parse_from(std::iter::once("example").chain(arguments.iter().copied()))
}

fn inherited(arguments: &[&str]) -> Result<Vec<OsString>, TestError> {
    let all = std::iter::once("example").chain(arguments.iter().copied());
    Ok(ExampleArguments::inherited(
        &Options::command().try_get_matches_from(all)?,
    ))
}

#[test]
fn a_second_process_takes_over_every_option_but_the_exit_time() -> Result<(), TestError> {
    let given = [
        "--text-scale",
        "1.5",
        "--reduced-motion",
        "--high-contrast",
        "--full-redraw",
    ];
    let args = inherited(&[&given[..], &["--exit-after", "3"]].concat())?;
    assert_eq!(args, given.map(OsString::from));
    let again: Vec<_> = args.iter().filter_map(|arg| arg.to_str()).collect();
    assert_eq!(inherited(&again)?, args);
    Ok(())
}

#[test]
fn invalid_values_are_rejected() {
    for arguments in [
        &["--width=-1", "--height=1"][..],
        &["--width=1", "--height=wide"],
        &["--text-scale=0"],
        &["--exit-after=-1"],
        &["--frames=out", "--frame-rate=0"],
        &["--frames=out", "--frame-rate=-30"],
        &["--frames=out", "--frame-rate=inf"],
        &["--frames=out", "--frame-rate=1e10"],
    ] {
        assert!(parse(arguments).is_err(), "{arguments:?} was accepted");
    }
}

#[test]
fn a_size_takes_both_sides_or_none() -> Result<(), TestError> {
    assert!(parse(&["--width", "1024"]).is_err());
    assert_eq!(parse(&[])?.size.size(), None);
    let parsed = parse(&["--width", "1024", "--height", "600"])?;
    assert_eq!(
        parsed.size.size(),
        Some(Size::new(Dp::new(1024.0), Dp::new(600.0)))
    );
    Ok(())
}

#[test]
fn a_recording_takes_a_directory_and_a_frame_rate() -> Result<(), TestError> {
    assert!(parse(&["--frames", "out"]).is_err());
    assert!(parse(&["--frame-rate", "30"]).is_err());
    let recording = parse(&["--frames", "out", "--frame-rate", "4"])?
        .recording
        .recording()
        .ok_or(TestError::NoRecording)?;
    assert_eq!(recording.directory, PathBuf::from("out"));
    assert_eq!(recording.interval.get(), Duration::from_millis(250));
    Ok(())
}
