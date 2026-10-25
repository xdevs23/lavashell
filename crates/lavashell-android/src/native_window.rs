// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::platform::ndk;
use lavashell_renderer::SurfaceTiming;

/// The producer end of a buffer queue, together with the timing history of the Surface of that queue. A `NativeWindow`
/// does not extend the lifetime of the consumer of a `SurfaceView`. Detach the renderer before the callback of the host
/// for the destruction of the surface returns.
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
