// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, ThemeChoice};
use clap::{ArgMatches, Args, Command, parser::ValueSource};
use lavashell::prelude::*;
use lavashell::renderer::DamageMode;
use lavashell_test_platform::FrameInterval;
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    time::Duration,
};

#[derive(clap::Args, Clone, Debug)]
pub struct ExampleArguments {
    #[command(flatten)]
    inherited: InheritedArguments,
    #[arg(long, value_parser = seconds)]
    exit_after: Option<Duration>,
}

#[derive(clap::Args, Clone, Debug)]
struct InheritedArguments {
    #[arg(long, value_parser = text_scale)]
    text_scale: Option<TextScaling>,
    #[command(flatten)]
    preferences: PreferenceArguments,
    #[arg(long)]
    full_redraw: bool,
    #[arg(long, value_enum, default_value_t)]
    theme: ThemeChoice,
}

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

    #[must_use]
    pub fn theme(&self) -> ThemeChoice {
        self.inherited.theme
    }

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

    pub fn app(&self) -> Result<App, Error> {
        let mut app = App::new(crate::platform())
            .text_transfers()
            .fonts(crate::fonts()?)
            .preference_overrides(self.preference_overrides());
        if self.inherited.full_redraw {
            app = app.damage(DamageMode::Whole).configure_painter(|painter| {
                painter.set_packet_caching(lavashell::renderer::PacketCaching::Disabled);
            });
        }
        Ok(match self.exit_after {
            Some(duration) => app.exit_after(duration),
            None => app,
        })
    }
}

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

    #[must_use]
    pub fn window(self, window: Window) -> Window {
        match self.size() {
            Some(size) => window.size(size.width, size.height),
            None => window,
        }
    }
}

#[derive(clap::Args, Clone, Debug)]
pub struct RecordingArguments {
    #[arg(
        long,
        value_name = "DIRECTORY",
        requires = "frame_rate",
        help = "Write each recorded frame as a numbered PNG file into this directory"
    )]
    frames: Option<PathBuf>,
    #[arg(
        long,
        value_name = "HZ",
        value_parser = frame_interval,
        requires = "frames",
        help = "Frames per second of the recording"
    )]
    frame_rate: Option<FrameInterval>,
}

pub struct Recording {
    pub directory: PathBuf,
    pub interval: FrameInterval,
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

fn number(value: &str) -> Result<f64, Error> {
    value.parse::<f64>().map_err(Error::Number)
}

fn text_scale(value: &str) -> Result<TextScaling, Error> {
    let factor = number(value)?;
    TextScaling::linear(factor).ok_or(Error::TextScale)
}

fn seconds(value: &str) -> Result<Duration, Error> {
    let seconds = number(value)?;
    Duration::try_from_secs_f64(seconds).map_err(Error::Seconds)
}

fn length(value: &str) -> Result<NonNegativeDp, Error> {
    let length = number(value)?;
    NonNegativeDp::try_new(Dp::new(length)).map_err(Error::Length)
}

fn frame_interval(value: &str) -> Result<FrameInterval, Error> {
    let rate = number(value)?;
    if !(rate.is_finite() && rate > 0.0) {
        return Err(Error::FrameRate);
    }
    let interval = Duration::try_from_secs_f64(rate.recip()).map_err(Error::Seconds)?;
    FrameInterval::new(interval).ok_or(Error::FrameRate)
}

#[cfg(test)]
mod tests;
