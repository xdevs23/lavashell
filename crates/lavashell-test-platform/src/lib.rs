// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod capture;
mod clock;
mod error;
mod frame_interval;
mod platform;
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
pub use frame_interval::FrameInterval;
pub use platform::{ScriptedHost, TestPlatform};
pub use recording::{RecordedFrame, RecordedFrames};
pub use rest::is_at_rest;
pub use script::HostScript;
