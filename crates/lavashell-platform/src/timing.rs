// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::{FrameTime, PresentationFlags};
use lavashell_core::surface::SurfaceId;
use std::time::Duration;

/// What the host reports about the frames of a surface. A platform converts each time to the clock of the app when
/// the report arrives.
#[derive(Clone, Copy, Debug)]
pub enum TimingEvent {
    /// The display is ready for the next frame of the surface.
    DisplayReady(SurfaceId),
    /// The frame drawn for `frame` became visible. `time` is `None` when the clock of the host cannot be converted to
    /// the clock of the app. `refresh` is `None` for an unknown or variable refresh interval.
    Presented {
        surface: SurfaceId,
        frame: FrameTime,
        time: Option<Duration>,
        refresh: Option<Duration>,
        flags: PresentationFlags,
    },
    /// The frame drawn for `frame` was never shown, because a later frame replaced it.
    Discarded {
        surface: SurfaceId,
        frame: FrameTime,
    },
}
