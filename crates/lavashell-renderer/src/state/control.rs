// SPDX-License-Identifier: GPL-3.0-only
use crate::{FrameOutcome, Snapshot};
use lavashell_core::prelude::{FrameTime, PixelScale, Viewport};
use lavashell_core::surface::SurfaceState;
use std::time::Duration;

/// The frame drawn by the renderer state into a surface, with the state of the surface, applied by the host together
/// with the frame.
#[derive(Clone, Copy, Debug)]
pub struct PresentFrame<'a> {
    pub time: FrameTime,
    pub viewport: Viewport,
    /// The number of output pixels covered by one viewport pixel. The whole scene is drawn at this scale.
    pub scale: PixelScale,
    /// The presentation time set on the buffer of the frame, on the clock of `VK_GOOGLE_display_timing`, from 1 to
    /// `i64::MAX` nanoseconds. `None` lets the presentation engine choose. After one frame with a time, every later
    /// frame of the surface needs one, as `SurfaceTiming` describes.
    pub present_at: Option<Duration>,
    pub state: &'a SurfaceState,
}

/// The requests of a platform that belong to the draw of each frame of one surface.
pub trait SurfaceControl {
    type Error: std::error::Error + Send + 'static;
    /// What the platform sends to the control between frames, for example a report from the host that a later frame
    /// applies.
    type Message: Send;

    fn receive(&mut self, message: Self::Message);

    /// Runs before every draw of `frame`, whether or not the frame reaches present.
    ///
    /// # Errors
    ///
    /// Fails when the platform cannot make a request for `frame`.
    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), Self::Error>;

    /// Runs once the surface has an image for `frame`, right before the frame is painted and presented. A request
    /// made here applies with the present of `frame`.
    fn before_present(&mut self, frame: &PresentFrame);

    /// Runs once `frame` is painted, before it is presented. The default does nothing.
    ///
    /// # Errors
    ///
    /// Fails when the platform cannot use the painted frame.
    fn painted(
        &mut self,
        _frame: &PresentFrame,
        _snapshot: Snapshot<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Runs after each draw with its outcome. Only a `Shown` frame was presented.
    fn after_present(&mut self, outcome: FrameOutcome);

    /// Makes the host apply `state` at once, without a new frame.
    ///
    /// # Errors
    ///
    /// Fails when the platform cannot make a request for `state`.
    fn commit_state(&mut self, state: &SurfaceState) -> Result<(), Self::Error>;
}
