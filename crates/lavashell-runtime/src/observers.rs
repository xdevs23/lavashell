// SPDX-License-Identifier: GPL-3.0-only
use crate::cpu_time::CpuTiming;
use crate::failures::log_failure;
use crate::view_actor::ComposedFrame;
use lavashell_core::frame::{FrameReport, FrameTime};
use lavashell_core::geometry::Viewport;
use lavashell_core::output::OutputInfo;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::{ComponentFailure, Failures, View};
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

type ViewObserver = Box<dyn FnMut(SurfaceId, FrameTime, Viewport, &View)>;

#[derive(Default)]
pub struct Observers {
    pub tick: Option<Box<dyn FnMut(Duration)>>,
    pub frame: Option<Box<dyn FnMut(FrameTime)>>,
    pub frame_report: Option<Box<dyn FnMut(FrameReport)>>,
    pub key: Option<Box<dyn FnMut(HostKey) -> bool>>,
    pub keyboard_focus: Option<Box<dyn FnMut(KeyboardFocus)>>,
    pub view: Option<ViewObserver>,
    pub output: Option<Box<dyn FnMut(SurfaceId, OutputInfo)>>,
    pub failure: Option<Box<dyn FnMut(SurfaceId, ComponentFailure)>>,
}

impl Observers {
    pub(crate) fn cpu_timing(&self) -> CpuTiming {
        match self.frame_report {
            Some(_) => CpuTiming::Measured,
            None => CpuTiming::Skipped,
        }
    }

    pub(crate) fn observe_view(&mut self, surface: SurfaceId, frame: ComposedFrame, view: &View) {
        if let Some(observer) = &mut self.view {
            observer(surface, frame.timing, frame.viewport, view);
        }
    }

    pub(crate) fn report_failures(&mut self, surface: SurfaceId, failures: Failures) {
        for failure in failures {
            log_failure(&failure);
            if let Some(observer) = &mut self.failure {
                observer(surface, failure);
            }
        }
    }
}
