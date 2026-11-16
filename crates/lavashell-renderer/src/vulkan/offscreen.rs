// SPDX-License-Identifier: GPL-3.0-only
use lavashell_vulkan::Device;
use skia_safe::Surface;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct OffscreenSurface {
    surface: Surface,
    device: Arc<Device>,
}

impl OffscreenSurface {
    pub(super) fn new(surface: Surface, device: Arc<Device>) -> Self {
        Self { surface, device }
    }

    pub(super) fn belongs_to(&self, device: &Arc<Device>) -> bool {
        Arc::ptr_eq(&self.device, device)
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
