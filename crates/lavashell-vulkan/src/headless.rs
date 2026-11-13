// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, ExtensionSet, Instance, Window};
use ash::vk;

/// A window without a window system, whose surfaces present through `VK_EXT_headless_surface` and show nothing.
pub struct HeadlessWindow;

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new headless surface of `instance`, and only the `Surface` that receives it destroys it.
// A headless surface has no native window that could be destroyed before it.
unsafe impl Window for HeadlessWindow {
    fn create_surface(&self, instance: &Instance) -> Result<vk::SurfaceKHR, Error> {
        instance.require(&[ash::ext::headless_surface::NAME])?;
        let functions = ash::ext::headless_surface::Instance::new(instance.entry(), instance.ash());
        let info = vk::HeadlessSurfaceCreateInfoEXT::default();
        // SAFETY: ash 0.38 has no safe form of vkCreateHeadlessSurfaceEXT. The instance is alive and has the extension
        // enabled, as `require` checked, and `info` lives through the call.
        unsafe { functions.create_headless_surface(&info, None) }.map_err(Error::CreateSurface)
    }
}
