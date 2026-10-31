// SPDX-License-Identifier: GPL-3.0-only
//! Reports sent by a host to the frame observer registered for an app when a frame is submitted, presented or
//! discarded.

use super::{FrameMetrics, FrameTime};
use crate::surface::SurfaceId;
use crate::view::ViewStats;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FrameStage {
    Submitted,
    Presented,
    Discarded,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameReport {
    pub surface: SurfaceId,
    pub timing: FrameTime,
    pub stage: FrameStage,
    /// The time of the presentation, measured from the start of the app. For a discarded frame, or when the host
    /// clock cannot be converted to the app clock, this is the time at which the feedback arrived.
    pub observed: Duration,
    /// `None` when only the arrival time of the feedback is known. A report without this time cannot be used to
    /// measure display timing or the error of an animation.
    pub presentation_time: Option<Duration>,
    pub metrics: Option<FrameMetrics>,
    pub flags: Option<PresentationFlags>,
    pub submission: Option<Submission>,
}

#[derive(Clone, Copy, Debug)]
pub struct Submission {
    /// The wall-clock time spent on composition, painting and submission, including time in which the thread was
    /// preempted.
    pub render_cost: Duration,
    /// The CPU time of the calling thread within the same span, without the time of other threads or of the GPU.
    pub cpu_time: Duration,
    pub view: ViewStats,
    pub render: RenderStats,
    /// The generation of the scene image drawn beneath the view.
    pub scene: Option<u64>,
    /// When GPU timing is enabled on the app, the GPU time of the latest frame on this surface that was measured
    /// after the previous report.
    pub gpu: Option<GpuFrameTime>,
}

/// One property of how the host presented a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationFlag {
    /// The presentation was synchronized to the vertical retrace of the display.
    Vsync,
    /// The timestamp comes from the clock of the display hardware.
    HardwareClock,
    /// The display hardware signalled that the presentation completed.
    HardwareCompletion,
    /// The display scanned the buffer out without a copy.
    ZeroCopy,
}

impl PresentationFlag {
    fn slot(self) -> usize {
        match self {
            Self::Vsync => 0,
            Self::HardwareClock => 1,
            Self::HardwareCompletion => 2,
            Self::ZeroCopy => 3,
        }
    }
}

/// How the host presented a frame: the set of properties that apply to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationFlags([bool; 4]);

impl PresentationFlags {
    #[must_use]
    pub fn contains(self, flag: PresentationFlag) -> bool {
        self.0[flag.slot()]
    }
}

impl FromIterator<PresentationFlag> for PresentationFlags {
    fn from_iter<T: IntoIterator<Item = PresentationFlag>>(flags: T) -> Self {
        let mut set = Self::default();
        for flag in flags {
            set.0[flag.slot()] = true;
        }
        set
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub uncached_leaves: u64,
    pub skipped_leaves: u64,
    pub cache_bytes: usize,
    pub damaged_pixels: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuFrameTime {
    pub sequence: u64,
    pub elapsed: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_set_of_one_flag_contains_only_that_flag() {
        const ALL: [PresentationFlag; 4] = [
            PresentationFlag::Vsync,
            PresentationFlag::HardwareClock,
            PresentationFlag::HardwareCompletion,
            PresentationFlag::ZeroCopy,
        ];
        for flag in ALL {
            let flags: PresentationFlags = [flag].into_iter().collect();
            for other in ALL {
                assert_eq!(flags.contains(other), other == flag);
            }
        }
    }
}
