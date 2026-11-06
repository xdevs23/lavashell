// SPDX-License-Identifier: GPL-3.0-only
use super::presentation::Feedback;
use super::{FrameCallbackData, FrameOutcome, Platform, QueueHandle, WpViewport, wl_surface};
use crate::ControlError;
use lavashell_core::prelude::{Density, Viewport};
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::{PresentFrame, SurfaceControl};
use smithay_client_toolkit::reexports::protocols::wp::presentation_time::client::wp_presentation::WpPresentation;

/// The requests that the draw of each frame of one native surface applies.
pub(super) struct Control {
    surface: SurfaceId,
    wl: wl_surface::WlSurface,
    viewport: Option<WpViewport>,
    presentation: Option<WpPresentation>,
    qh: QueueHandle<Platform>,
    /// Whether a frame callback was requested for a frame that was not shown. The next shown frame commits it.
    frame_requested: bool,
}

impl Control {
    pub fn new(
        surface: SurfaceId,
        wl: wl_surface::WlSurface,
        viewport: Option<WpViewport>,
        presentation: Option<WpPresentation>,
        qh: QueueHandle<Platform>,
    ) -> Self {
        Self {
            surface,
            wl,
            viewport,
            presentation,
            qh,
            frame_requested: false,
        }
    }

    pub fn destroy(self) {
        if let Some(viewport) = self.viewport {
            viewport.destroy();
        }
    }
}

impl SurfaceControl for Control {
    type Error = ControlError;

    /// Sets the size of the surface in logical pixels.
    fn prepare(&mut self, frame: &PresentFrame) -> Result<(), ControlError> {
        let Viewport { size, density } = frame.viewport;
        match &self.viewport {
            Some(viewport) => {
                let (width, height) =
                    crate::logical::protocol_size(size).ok_or(ControlError::Destination(size))?;
                viewport.set_destination(width, height);
            }
            None => self.wl.set_buffer_scale(buffer_scale(density)?),
        }
        Ok(())
    }

    /// Requests the frame callback and the presentation feedback that the present of `frame` commits.
    fn before_present(&mut self, frame: &PresentFrame) {
        if !self.frame_requested {
            let data = FrameCallbackData(self.wl.clone());
            let _dispatched_on_done = self.wl.frame(&self.qh, data);
            self.frame_requested = true;
        }
        if let Some(presentation) = &self.presentation {
            let feedback = Feedback {
                surface: self.surface,
                timing: frame.time,
            };
            let _dispatched_on_presented = presentation.feedback(&self.wl, &self.qh, feedback);
        }
    }

    fn after_present(&mut self, outcome: FrameOutcome) {
        if outcome == FrameOutcome::Shown {
            self.frame_requested = false;
        }
    }
}

/// The whole number of buffer pixels along one logical pixel at `density`. A surface without a viewport sets it
/// with `wl_surface.set_buffer_scale`, which accepts only whole numbers.
fn buffer_scale(density: Density) -> Result<i32, ControlError> {
    let unit = crate::logical::size((1, 1));
    let (scale, _) = unit
        .buffer_size(density)
        .map_err(ControlError::BufferSize)?;
    i32::try_from(scale).map_err(ControlError::BufferScale)
}
