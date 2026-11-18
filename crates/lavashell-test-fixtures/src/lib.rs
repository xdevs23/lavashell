// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod frames;
mod scene;
pub mod test_bus;
mod text;

pub use frames::{FrameFailure, WithoutFailures, failure_causes};
pub use lavashell_core::platform::fixture::{
    DOUBLE_PRESS, LONG_PRESS, REPEAT_DELAY, REPEAT_INTERVAL, facts,
};
pub use lavashell_core::scene::tree::show_all;
pub use lavashell_core::view::fixture::press_and_release_at;
pub use scene::RecordedScene;
pub use text::NoText;
