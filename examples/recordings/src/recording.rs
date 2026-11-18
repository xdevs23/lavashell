// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_example_support::parse_number;
use lavashell_test_platform::FrameInterval;
use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::Args, Clone, Debug)]
pub struct Recording {
    #[arg(
        long = "frames",
        value_name = "DIRECTORY",
        help = "Write each recorded frame as a numbered PNG file into this directory"
    )]
    pub directory: PathBuf,
    #[arg(
        long = "frame-rate",
        value_name = "HZ",
        value_parser = frame_interval,
        help = "Frames per second of the recording"
    )]
    pub interval: FrameInterval,
}

fn frame_interval(value: &str) -> Result<FrameInterval, Error> {
    let rate = parse_number(value)?;
    let interval = Duration::try_from_secs_f64(rate.recip())
        .map_err(|source| Error::FrameRateNotPositive { rate, source })?;
    FrameInterval::new(interval).ok_or(Error::FrameRateAboveOnePerNanosecond { rate })
}

#[cfg(test)]
mod tests;
