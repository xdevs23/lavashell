// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameOutcome;
use lavashell_core::prelude::{FrameTime, Viewport};
use std::time::Duration;

/// The frame that the renderer state draws into a surface.
#[derive(Clone, Copy, Debug)]
pub struct PresentFrame {
    pub time: FrameTime,
    pub viewport: Viewport,
    /// The presentation time set on the buffer of the frame, on the clock of `VK_GOOGLE_display_timing`, from 1 to
    /// `i64::MAX` nanoseconds. `None` lets the presentation engine choose. After one frame with a time, every later
    /// frame of the surface needs one, as `SurfaceTiming` describes.
    pub present_at: Option<Duration>,
}

/// The requests of a platform that belong to the draw of each frame of one surface.
pub trait SurfaceControl {
    type Error: std::error::Error + 'static;

    /// Runs before every draw of `frame`, whether or not the frame reaches present.
    ///
    /// # Errors
    ///
    /// Fails when the platform cannot make a request for `frame`.
    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), Self::Error>;

    /// Runs once the surface has an image for `frame`, right before the frame is painted and presented. A request
    /// made here applies with the present of `frame`.
    fn before_present(&mut self, frame: &PresentFrame);

    /// Runs after each draw with its outcome. Only a `Shown` frame was presented.
    fn after_present(&mut self, outcome: FrameOutcome);
}
