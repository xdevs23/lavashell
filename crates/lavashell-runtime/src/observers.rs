// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::CpuTiming;
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::Viewport;
use lavashell_core::output::OutputInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use std::time::Duration;

pub use lavashell_platform::KeyAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub surface: SurfaceId,
    pub code: u32,
    pub action: KeyAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardFocus {
    pub surface: SurfaceId,
    pub focused: bool,
}

type ViewObserver = Box<dyn FnMut(SurfaceId, Viewport, &View)>;

#[derive(Default)]
pub struct Observers {
    pub tick: Option<Box<dyn FnMut(Duration)>>,
    pub frame: Option<Box<dyn FnMut(FrameTime)>>,
    pub frame_report: Option<Box<dyn FnMut(FrameReport)>>,
    pub key: Option<Box<dyn FnMut(HostKey) -> bool>>,
    pub keyboard_focus: Option<Box<dyn FnMut(KeyboardFocus)>>,
    pub view: Option<ViewObserver>,
    pub output: Option<Box<dyn FnMut(SurfaceId, OutputInfo)>>,
}

impl Observers {
    pub(crate) fn cpu_timing(&self) -> CpuTiming {
        match self.frame_report {
            Some(_) => CpuTiming::Measured,
            None => CpuTiming::Skipped,
        }
    }
}
