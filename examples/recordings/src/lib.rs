// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod capture;
mod error;
mod frame_recorder;
pub mod keyboard;
pub mod materials;
pub mod music_player;
mod recording;
mod recording_platform;
mod steps;
pub mod taskbar;

pub use capture::{capture_path, write_capture, write_first_frames, write_screenshot};
pub use error::Error;
pub use lavashell_test_platform::{FrameInterval, RecordedFrame};
pub use recording::Recording;
pub use recording_platform::RecordingPlatform;
pub use steps::{Start, Steps, at_rest};
