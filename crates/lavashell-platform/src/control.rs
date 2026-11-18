// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::{FrameTime, ScaledViewport};
use lavashell_core::surface::SurfaceState;
use lavashell_renderer::skia::Canvas;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct PresentFrame<'a> {
    pub time: FrameTime,
    pub scaled_viewport: ScaledViewport,
    pub present_at: Option<Duration>,
    pub state: &'a SurfaceState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameOutcome {
    Shown,
    Pending,
    Stale,
}

pub trait SurfaceControl {
    type Error: std::error::Error + Send + 'static;
    type Message: Send;

    fn receive(&mut self, message: Self::Message);

    fn prepare(&mut self, _frame: &PresentFrame) -> Result<(), Self::Error> {
        Ok(())
    }

    fn before_present(&mut self, _frame: &PresentFrame) {}

    fn painted(&mut self, _frame: &PresentFrame, _canvas: &Canvas) -> Result<(), Self::Error> {
        Ok(())
    }

    fn after_present(&mut self, _outcome: FrameOutcome) {}

    fn commit_state(&mut self, _state: &SurfaceState) -> Result<(), Self::Error> {
        Ok(())
    }
}
