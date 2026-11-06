// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::platform::ndk;
use lavashell_core::surface::SurfaceState;
use lavashell_renderer::{FrameOutcome, PresentFrame, SurfaceControl, SurfaceTiming};
use std::convert::Infallible;

/// The producer end of a buffer queue, together with the timing history of the Surface of that queue. A `NativeWindow`
/// does not extend the lifetime of the consumer of a `SurfaceView`. Detach the renderer before the callback of the host
/// for the destruction of the surface returns.
///
/// The policy of `SurfaceTiming` applies to the Surface also after the Surface is detached and attached again. Every
/// native window acquired from the Surface therefore shares one `SurfaceTiming`, and a Surface with an unknown history
/// starts with `PresentationPolicy::Explicit`.
pub struct NativeWindow {
    pub(crate) window: ndk::native_window::NativeWindow,
    pub(crate) timing: SurfaceTiming,
}

impl NativeWindow {
    /// `timing` must be the timing history of this Surface across every native window acquired from it.
    /// `SurfaceTiming` describes that history.
    #[must_use]
    pub fn new(window: ndk::native_window::NativeWindow, timing: SurfaceTiming) -> Self {
        Self { window, timing }
    }

    /// The width and the height of the consumer in pixels, as reported by the native window. A buffer geometry set
    /// explicitly on the window can give the Vulkan images another size without changing this width and height.
    ///
    /// # Errors
    /// Fails when AOSP reports a failure status instead of the dimensions.
    pub fn size(&self) -> Result<(u32, u32), Error> {
        let (width, height) = (self.window.width(), self.window.height());
        match (u32::try_from(width), u32::try_from(height)) {
            (Ok(width), Ok(height)) => Ok((width, height)),
            _ => Err(Error::WindowSize { width, height }),
        }
    }
}

/// `PresentFrame` sets the presentation time, and a Surface has no surface state, so a native window sends nothing.
impl SurfaceControl for NativeWindow {
    type Error = Infallible;
    type Message = Infallible;

    fn receive(&mut self, message: Infallible) {
        match message {}
    }

    fn prepare(&mut self, _: &PresentFrame) -> Result<(), Infallible> {
        Ok(())
    }

    fn before_present(&mut self, _: &PresentFrame) {}

    fn after_present(&mut self, _: FrameOutcome) {}

    fn commit_state(&mut self, _: &SurfaceState) -> Result<(), Infallible> {
        Ok(())
    }
}
