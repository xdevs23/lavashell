// SPDX-License-Identifier: GPL-3.0-only
//! A platform that runs an app without a display, on a virtual clock advanced by a script. The script reports
//! events to the app, steps through time and records the frames shown at each step as PNG images.
#![forbid(unsafe_code)]

mod capture;
mod clock;
mod error;
mod frame_interval;
mod pace;
mod platform;
mod recording;
mod rest;
mod script;
mod session;
#[cfg(test)]
mod tests;

pub use capture::FrameCapture;
pub use error::Error;
pub use frame_interval::{FrameInterval, FrameRateError};
pub use pace::{Pace, StepsPace};
pub use platform::TestPlatform;
pub use recording::{RecordedFrame, RecordedFrames};
pub use rest::is_at_rest;
pub use script::HostScript;
