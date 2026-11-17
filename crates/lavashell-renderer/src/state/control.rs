// SPDX-License-Identifier: GPL-3.0-only
use crate::{FrameOutcome, Snapshot};
use lavashell_core::prelude::{Affine, FrameTime, PixelScale, Viewport};
use lavashell_core::surface::SurfaceState;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct PresentFrame<'a> {
    pub time: FrameTime,
    pub viewport: Viewport,
    pub scale: PixelScale,
    pub present_at: Option<Duration>,
    pub state: &'a SurfaceState,
}

impl PresentFrame<'_> {
    #[must_use]
    pub fn dp_to_output(&self) -> Affine {
        self.scale.dp_to_output(self.viewport.density)
    }
}

pub trait SurfaceControl {
    type Error: std::error::Error + Send + 'static;
    type Message: Send;

    fn receive(&mut self, message: Self::Message);

    fn prepare(&mut self, _frame: &PresentFrame) -> Result<(), Self::Error> {
        Ok(())
    }

    fn before_present(&mut self, _frame: &PresentFrame) {}

    fn painted(
        &mut self,
        _frame: &PresentFrame,
        _snapshot: Snapshot<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn after_present(&mut self, _outcome: FrameOutcome) {}

    fn commit_state(&mut self, _state: &SurfaceState) -> Result<(), Self::Error> {
        Ok(())
    }
}
