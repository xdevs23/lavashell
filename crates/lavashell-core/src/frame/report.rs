// SPDX-License-Identifier: GPL-3.0-only

use super::{FrameMetrics, FrameTime};
use crate::flag_set::flag_set;
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
    pub observed: Duration,
    pub presentation_time: Option<Duration>,
    pub metrics: Option<FrameMetrics>,
    pub flags: Option<PresentationFlags>,
    pub submission: Option<Submission>,
}

#[derive(Clone, Copy, Debug)]
pub struct Submission {
    pub render_cost: Duration,
    pub cpu_time: Duration,
    pub view: ViewStats,
    pub render: RenderStats,
    pub scene: Option<u64>,
    pub gpu: Option<GpuFrameTime>,
}

flag_set!(
    pub enum PresentationFlag in PresentationFlags {
        Vsync,
        HardwareClock,
        HardwareCompletion,
        ZeroCopy,
    }
);

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
        for flag in PresentationFlag::ALL {
            let flags: PresentationFlags = [flag].into_iter().collect();
            for other in PresentationFlag::ALL {
                assert_eq!(flags.contains(other), other == flag);
            }
        }
    }
}
