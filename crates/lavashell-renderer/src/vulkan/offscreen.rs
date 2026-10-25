// SPDX-License-Identifier: GPL-3.0-only
use lavashell_vulkan::Device;
use skia_safe::Surface;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// A Skia surface in the memory of the Vulkan device used by a renderer. Skia frees the last device memory of its
/// context only once the last surface of that context is gone. The surface therefore has a reference to the device,
/// so the device is not destroyed before that memory is freed, also after the renderer is dropped.
pub struct OffscreenSurface {
    /// Declared before the device, so the surface drops first.
    surface: Surface,
    _device: Arc<Device>,
}

impl OffscreenSurface {
    pub(super) fn new(surface: Surface, device: Arc<Device>) -> Self {
        Self {
            surface,
            _device: device,
        }
    }
}

impl Deref for OffscreenSurface {
    type Target = Surface;

    fn deref(&self) -> &Surface {
        &self.surface
    }
}

impl DerefMut for OffscreenSurface {
    fn deref_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }
}
