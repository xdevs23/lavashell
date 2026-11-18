// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell_example_recordings::{Error, Recording, keyboard, materials, music_player, taskbar};
use lavashell_example_support::{ExampleArguments, SizeArguments, ThemeChoice, exit_code};
use lavashell_materials_example::OutputArguments;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
enum RecordedExample {
    Keyboard {
        #[command(flatten)]
        recording: Recording,
        #[arg(long, value_enum, default_value_t)]
        theme: ThemeChoice,
    },
    Taskbar {
        #[command(flatten)]
        recording: Recording,
        #[arg(long, value_enum, default_value_t)]
        theme: ThemeChoice,
        #[command(flatten)]
        size: SizeArguments,
    },
    Materials {
        #[command(flatten)]
        example: ExampleArguments,
        #[command(flatten)]
        output: OutputArguments,
        #[arg(long, value_name = "FILE")]
        screenshot: PathBuf,
    },
    MusicPlayer {
        #[command(flatten)]
        example: ExampleArguments,
        #[command(flatten)]
        size: SizeArguments,
        #[arg(long, value_name = "FILE")]
        screenshot: PathBuf,
    },
}

fn main() -> ExitCode {
    exit_code(record(RecordedExample::parse()))
}

fn record(example: RecordedExample) -> Result<(), Error> {
    match example {
        RecordedExample::Keyboard { recording, theme } => keyboard::record(&recording, theme),
        RecordedExample::Taskbar {
            recording,
            theme,
            size,
        } => taskbar::record(&recording, size.size(), theme),
        RecordedExample::Materials {
            example,
            output,
            screenshot,
        } => materials::screenshot(&example, output, &screenshot),
        RecordedExample::MusicPlayer {
            example,
            size,
            screenshot,
        } => music_player::screenshot(&example, size.size(), &screenshot),
    }
}

#[cfg(test)]
mod tests;
