// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod capture;
mod clock;
mod error;
mod frame_interval;
mod host_input;
mod linux_key_code;
mod pace;
mod platform;
mod record_requests;
mod recording;
mod rest;
mod screen;
mod script;
mod session;
#[cfg(test)]
mod tests;
mod windows;

pub use capture::FrameCapture;
pub use error::Error;
pub use frame_interval::{FrameInterval, FrameRateError};
pub use host_input::HostInput;
pub use pace::{LatestPace, Pace, PacedLinks};
pub use platform::{ScriptedHost, TestPlatform};
pub use record_requests::RecordRequests;
pub use recording::{RecordedFrame, RecordedFrames};
pub use rest::is_at_rest;
pub use script::HostScript;
