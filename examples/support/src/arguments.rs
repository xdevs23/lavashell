// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use clap::{ArgMatches, Args, Command, parser::ValueSource};
use lavashell::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    time::Duration,
};

/// The command-line options accepted by every windowed example.
#[derive(clap::Args, Clone, Debug)]
pub struct ExampleArguments {
    #[command(flatten)]
    inherited: InheritedArguments,
    #[arg(long, value_parser = seconds)]
    exit_after: Option<Duration>,
}

/// The command-line options that an example passes on when it starts a second process of itself, such as a child
/// window. The second process then uses the same text scale, accessibility preferences and redraw mode.
#[derive(clap::Args, Clone, Debug)]
struct InheritedArguments {
    #[arg(long, value_parser = text_scale)]
    text_scale: Option<TextScaling>,
    #[command(flatten)]
    preferences: PreferenceArguments,
    #[arg(long)]
    full_redraw: bool,
}

/// The accessibility preferences set on the command line. Each flag turns its preference on, in place of the value
/// reported by the system.
#[derive(clap::Args, Clone, Copy, Debug)]
struct PreferenceArguments {
    #[arg(long)]
    reduced_motion: bool,
    #[arg(long)]
    reduced_transparency: bool,
    #[arg(long)]
    high_contrast: bool,
}

impl ExampleArguments {
    #[must_use]
    pub fn preference_overrides(&self) -> PreferenceOverrides {
        let preferences = self.inherited.preferences;
        PreferenceOverrides {
            text_scaling: self.inherited.text_scale.clone(),
            reduced_motion: preferences.reduced_motion.then_some(true),
            reduced_transparency: preferences.reduced_transparency.then_some(true),
            contrast: preferences
                .high_contrast
                .then_some(ContrastPreference::More),
        }
    }

    /// The options from `matches` to pass to a second process of the same example, in the form in which they
    /// appeared on the command line. These are all options except `--exit-after`. Options left at their default
    /// are skipped.
    #[must_use]
    pub fn inherited(matches: &ArgMatches) -> Vec<OsString> {
        let mut options = InheritedArguments::augment_args(Command::new("inherited"));
        options.build();
        options
            .get_arguments()
            .filter(|option| {
                let source = matches.value_source(option.get_id().as_str());
                source == Some(ValueSource::CommandLine)
            })
            .flat_map(|option| {
                let name = (option.get_long()).map(|long| OsString::from(format!("--{long}")));
                let values = (option.get_action().takes_values())
                    .then(|| matches.get_raw(option.get_id().as_str()))
                    .flatten();
                name.into_iter()
                    .chain(values.into_iter().flatten().map(OsStr::to_owned))
            })
            .collect()
    }

    /// # Errors
    /// Returns an error when the example fonts do not load.
    #[cfg(target_os = "linux")]
    pub fn app(&self) -> Result<App, Error> {
        let full_redraw = self.inherited.full_redraw;
        let mut app = App::new(crate::platform())
            .text_transfers()
            .fonts(crate::fonts()?)
            .preference_overrides(self.preference_overrides())
            .incremental(!full_redraw);
        if full_redraw {
            app = app.configure_painter(|painter| {
                painter.set_packet_caching(lavashell::renderer::PacketCaching::Disabled);
            });
        }
        Ok(match self.exit_after {
            Some(duration) => app.exit_after(duration),
            None => app,
        })
    }
}

/// The `--width` and `--height` options, which are accepted only together. Without them, a window opens at the
/// size measured for its content.
#[derive(clap::Args, Clone, Copy, Debug)]
pub struct SizeArguments {
    #[arg(long, value_parser = length, requires = "height")]
    width: Option<NonNegativeDp>,
    #[arg(long, value_parser = length, requires = "width")]
    height: Option<NonNegativeDp>,
}

impl SizeArguments {
    #[must_use]
    pub fn size(self) -> Option<Size> {
        Some(Size::new(self.width?.get(), self.height?.get()))
    }

    /// `window` with the size set by `--width` and `--height`. Without these options, `window` is returned unchanged
    /// and opens at the size measured for its content.
    #[cfg(target_os = "linux")]
    #[must_use]
    pub fn window(self, window: Window) -> Window {
        match self.size() {
            Some(size) => window.size(size.width, size.height),
            None => window,
        }
    }
}

/// The command-line options of an example that renders an animation offscreen into PNG files.
#[derive(clap::Args, Clone, Debug)]
pub struct RecordingArguments {
    /// Writes each recorded frame as a numbered PNG file into this directory.
    #[arg(long, value_name = "DIRECTORY", requires = "frame_rate")]
    frames: Option<PathBuf>,
    /// The frames per second of the recording.
    #[arg(long, value_name = "HZ", value_parser = frame_interval, requires = "frames")]
    frame_rate: Option<FrameInterval>,
}

/// The directory into which a recording writes its PNG files, and the time between two consecutive frames.
pub struct Recording {
    pub directory: PathBuf,
    pub interval: FrameInterval,
}

/// The time between two frames of a recording, which is longer than zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInterval(Duration);

impl FrameInterval {
    #[must_use]
    pub fn new(interval: Duration) -> Option<Self> {
        (!interval.is_zero()).then_some(Self(interval))
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}

impl RecordingArguments {
    #[must_use]
    pub fn recording(&self) -> Option<Recording> {
        Some(Recording {
            directory: self.frames.clone()?,
            interval: self.frame_rate?,
        })
    }
}

fn text_scale(value: &str) -> Result<TextScaling, Error> {
    let factor = value.parse::<f64>().map_err(Error::Number)?;
    TextScaling::linear(factor).ok_or(Error::TextScale)
}

fn seconds(value: &str) -> Result<Duration, Error> {
    let seconds = value.parse::<f64>().map_err(Error::Number)?;
    Duration::try_from_secs_f64(seconds).map_err(Error::Seconds)
}

fn length(value: &str) -> Result<NonNegativeDp, Error> {
    let length = value.parse::<f64>().map_err(Error::Number)?;
    NonNegativeDp::try_new(Dp::new(length)).map_err(Error::Length)
}

fn frame_interval(value: &str) -> Result<FrameInterval, Error> {
    let rate = value.parse::<f64>().map_err(Error::Number)?;
    if !(rate.is_finite() && rate > 0.0) {
        return Err(Error::FrameRate);
    }
    let interval = Duration::try_from_secs_f64(rate.recip()).map_err(Error::Seconds)?;
    FrameInterval::new(interval).ok_or(Error::FrameRate)
}

#[cfg(test)]
mod tests;
