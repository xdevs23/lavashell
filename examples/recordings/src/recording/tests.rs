// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use clap::Parser;

#[derive(Parser)]
struct Options {
    #[command(flatten)]
    recording: Recording,
}

fn parse(arguments: &[&str]) -> Result<Options, clap::Error> {
    Options::try_parse_from(std::iter::once("recordings").chain(arguments.iter().copied()))
}

#[test]
fn invalid_frame_rates_are_rejected() {
    for rate in ["0", "-30", "inf", "1e10", "fast"] {
        let parsed = parse(&["--frames", "out", "--frame-rate", rate]);
        assert!(parsed.is_err(), "{rate} was accepted");
    }
}

#[test]
fn a_recording_takes_a_directory_and_a_frame_rate() -> Result<(), clap::Error> {
    assert!(parse(&["--frames", "out"]).is_err());
    assert!(parse(&["--frame-rate", "30"]).is_err());
    let recording = parse(&["--frames", "out", "--frame-rate", "4"])?.recording;
    assert_eq!(recording.directory, PathBuf::from("out"));
    assert_eq!(recording.interval.get(), Duration::from_millis(250));
    Ok(())
}

#[test]
fn a_rate_of_zero_or_below_has_no_interval() {
    for rate in ["0", "-30"] {
        assert!(
            matches!(
                frame_interval(rate),
                Err(Error::FrameRateNotPositive { .. })
            ),
            "{rate}"
        );
    }
}

#[test]
fn a_rate_above_one_frame_per_nanosecond_is_refused() {
    for rate in ["1e10", "inf"] {
        assert!(
            matches!(
                frame_interval(rate),
                Err(Error::FrameRateAboveOnePerNanosecond { .. })
            ),
            "{rate}"
        );
    }
}
