// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::frame::{FrameTime, PresentationFlags};
use lavashell_core::surface::SurfaceId;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum TimingEvent {
    DisplayReady(SurfaceId),
    Presented {
        surface: SurfaceId,
        frame: FrameTime,
        time: Option<Duration>,
        refresh: Option<Duration>,
        flags: PresentationFlags,
    },
    Discarded {
        surface: SurfaceId,
        frame: FrameTime,
    },
}

impl Merge for TimingEvent {
    type Key = SurfaceId;

    fn effect(&self) -> MergeEffect<SurfaceId> {
        match *self {
            Self::DisplayReady(surface) => MergeEffect::Overwrites(surface),
            Self::Presented { .. } | Self::Discarded { .. } => MergeEffect::Appends,
        }
    }
}
